//! # Finalize a staged child-family observation
//!
//! The [`Archive`] methods here turn a reserved, staged acquisition into its terminal result.
//! [`ChildFamilyObservation`] carries thread/family identity, sequence, acquisition time,
//! completeness, expected page count, and optional pull-request head context. The convenience
//! method omits head context; context-aware methods support head-bound review families.
//!
//! Validation rejects unsupported families and inconsistent completeness/page-count declarations
//! before beginning the transaction. Complete reviews and review threads require head context;
//! other families reject supplied head context. These checks validate the declaration rather than
//! contacting the provider to verify the recorded head or acquisition time.
//!
//! The application module checks reservation ownership, recognizes completed replay, and loads
//! staged pages. Complete application validates the page set before replacing canonical membership
//! and coverage. Incomplete application verifies received counts and records the attempt without
//! erasing prior complete members. A superseded reservation can return a skipped result rather
//! than becoming a write error, so callers must inspect the disposition.
//!
//! This module owns the single transaction and commits after application succeeds. Fenced callers
//! must hold a current archive lease; unfenced methods do not enforce that authority. No
//! transaction crosses provider I/O. Application errors roll back this finalization, while staging
//! from earlier calls remains separately durable for recovery or retry.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{ObservationSequence, ThreadId};
use forgesync_core::observation::CollectionCompleteness;
use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::families::ChildFamilyObservation;
use crate::families::application::{FamilyApplication, FamilyFinalization};
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::{
    FamilyObservationResult, evidence_family_name, is_child_family, to_sql_sequence,
};

impl Archive {
    /// Commits a complete membership snapshot or records an incomplete attempt without replacing
    /// it.
    ///
    /// This convenience form supplies no head context, so complete reviews or review threads must
    /// use a context-aware method instead. It does not enforce an archive lease. Inspect the
    /// returned disposition: a superseded reservation is skipped without replacing membership.
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
    ///
    /// Authority is checked inside the finalization transaction. It does not prove provider
    /// freshness or reservation ownership; the application checks the reservation separately.
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
