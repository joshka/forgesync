//! # Finalize a staged child-family observation
//!
//! Finishing checks the reserved sequence and staged pages, then applies the collection outcome.
//! Complete membership may become canonical only after page validation; incomplete collection
//! still records the acquisition result without erasing earlier complete members.
//!
//! The engine calls this after pagination ends or fails. The store owns the transaction that makes
//! the outcome and membership agree, so a subsequent read never has to guess whether a
//! half-applied page set is authoritative.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{ObservationSequence, ThreadId};
use forgesync_core::observation::CollectionCompleteness;
use forgesync_core::timestamp::UtcTimestamp;

use super::ChildFamilyObservation;
use super::application::{FamilyApplication, FamilyFinalization};
use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::{
    FamilyObservationResult, evidence_family_name, is_child_family, to_sql_sequence,
};

impl Archive {
    /// Commits a complete membership snapshot or records an incomplete attempt without replacing
    /// it.
    pub async fn finish_child_family_observation(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        observed_at: UtcTimestamp,
        completeness: &CollectionCompleteness,
        expected_pages: Option<u32>,
    ) -> Result<FamilyObservationResult, StoreError> {
        let observation = ChildFamilyObservation {
            thread,
            family,
            sequence,
            observed_at,
            completeness,
            expected_pages,
            head_sha: None,
        };
        self.finish_child_family_observation_with_context(observation)
            .await
    }

    /// Finalizes a child family with its acquisition context and without an archive lease.
    pub async fn finish_child_family_observation_with_context(
        &self,
        observation: ChildFamilyObservation<'_>,
    ) -> Result<FamilyObservationResult, StoreError> {
        self.finish_child_family_observation_inner(observation, None)
            .await
    }

    /// Finalizes a child family only while the supplied archive lease remains current.
    pub async fn finish_child_family_observation_fenced(
        &self,
        observation: ChildFamilyObservation<'_>,
        token: &ArchiveLeaseToken,
    ) -> Result<FamilyObservationResult, StoreError> {
        self.finish_child_family_observation_inner(observation, Some(token))
            .await
    }

    /// Applies the terminal family result under one transaction and optional lease fencing.
    async fn finish_child_family_observation_inner(
        &self,
        observation: ChildFamilyObservation<'_>,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<FamilyObservationResult, StoreError> {
        observation.validate()?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence_value = to_sql_sequence(observation.sequence)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let finalization =
            FamilyApplication::prepare(&mut transaction, observation, sequence_value).await?;
        let result = match finalization {
            FamilyFinalization::Finished(result) => result,
            FamilyFinalization::Pending(application) => application.apply(&mut transaction).await?,
        };
        transaction.commit().await?;
        Ok(result)
    }
}

impl ChildFamilyObservation<'_> {
    /// Checks completeness and head requirements before any transaction can modify membership.
    fn validate(&self) -> Result<(), StoreError> {
        if !is_child_family(self.family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(self.family).to_owned(),
            ));
        }
        match self.completeness {
            CollectionCompleteness::Complete if self.expected_pages.is_none() => {
                return Err(StoreError::MissingExpectedPageCount);
            }
            CollectionCompleteness::Incomplete { .. } if self.expected_pages.is_some() => {
                return Err(StoreError::InvalidCollectionCompleteness);
            }
            _ => {}
        }
        let head_bound_family = matches!(
            self.family,
            EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads
        );
        if self.head_sha.is_some() && !head_bound_family {
            return Err(StoreError::UnexpectedPullRequestHeadContext);
        }
        if head_bound_family
            && matches!(self.completeness, CollectionCompleteness::Complete)
            && self.head_sha.is_none()
        {
            return Err(StoreError::MissingPullRequestHeadContext);
        }

        Ok(())
    }
}
