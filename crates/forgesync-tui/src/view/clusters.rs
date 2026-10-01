//! # Draw duplicate clusters and their decisions
//!
//! Cluster renderers show a page of suggested groups and the selected group's members, roles, and
//! local triage state. They consume stored projections already loaded into `App`.
//!
//! A proposal and a maintainer decision should look distinguishable. Layout here explains the
//! current cluster state; key handling and persistence remain in app input and query operations.
//!
//! `cluster_item` owns list-row wording. A borrowed `ClusterDetailView` keeps the loaded members
//! and selection together for heading and member-line rendering. Roles and inclusion states remain
//! distinct, scores are omitted when absent, and the marker and highlight identify the same member.
//! Drawing functions own loading/error precedence and widget placement; these projections start no
//! query and record no maintainer decision.

use forgesync_store::clusters::{
    ClusterDetail, ClusterLifecycle, ClusterMember, ClusterMemberRole, ClusterMemberState,
    ClusterSummary,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;
use crate::view::{PaneEmphasis, selected_style};

/// Draws the cluster list and its current selection.
pub fn draw_clusters(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut items: Vec<ListItem<'_>> = app
        .cluster_list
        .rows
        .data
        .iter()
        .map(cluster_item)
        .collect();
    if app.cluster_list.rows.loading && items.is_empty() {
        items.push(ListItem::new("Loading clusters…"));
    } else if let Some(error) = &app.cluster_list.rows.error {
        items = vec![ListItem::new(error.clone())];
    } else if items.is_empty() {
        items.push(ListItem::new(
            "No generated clusters in this repository scope",
        ));
    }
    let mut state = ListState::default();
    if app.cluster_list.rows.error.is_none() && !app.cluster_list.rows.data.is_empty() {
        state.select(Some(app.cluster_list.selected));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(PaneEmphasis::Strong.block("Generated clusters · neighbors"))
            .highlight_style(selected_style()),
        area,
        &mut state,
    );
}

/// Draws one cluster and its member decisions.
pub fn draw_cluster_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = if app.cluster_detail_pane.detail.loading
        && app.cluster_detail_pane.detail.data.is_none()
    {
        vec![Line::from("Loading cluster neighbors…")]
    } else if let Some(error) = &app.cluster_detail_pane.detail.error {
        vec![Line::from(error.clone())]
    } else if let Some(detail) = &app.cluster_detail_pane.detail.data {
        let view = ClusterDetailView {
            detail,
            selected_member: app.cluster_detail_pane.selected_member,
        };
        view.lines()
    } else {
        vec![Line::from(
            "Select a cluster to inspect its members and neighbor scores.",
        )]
    };
    if app.cluster_detail_pane.detail.loading && app.cluster_detail_pane.detail.data.is_some() {
        lines.insert(0, Line::from("Refreshing…"));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(PaneEmphasis::Strong.block("Cluster members and neighbors"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Formats one generated suggestion without confusing dismissal with generation lifecycle.
fn cluster_item(cluster: &ClusterSummary) -> ListItem<'static> {
    let lifecycle = match cluster.lifecycle {
        ClusterLifecycle::Active => "active",
        ClusterLifecycle::Retired => "retired",
    };
    let dismissed = if cluster.dismissed {
        " · dismissed"
    } else {
        ""
    };
    ListItem::new(format!(
        "{} · {} · {} active / {} excluded{dismissed}",
        cluster.title, lifecycle, cluster.active_member_count, cluster.excluded_member_count
    ))
}

/// Loaded cluster and its selected member, borrowed for one frame's presentation.
///
/// Selection is an index in this detail's ordered member projection. Application state validates
/// whether that selection can authorize an action; drawing only adds the visual marker and style.
struct ClusterDetailView<'a> {
    /// Loaded metadata and members, with their existing role and evidence score.
    detail: &'a ClusterDetail,
    /// Index whose line receives the selection marker and highlight.
    selected_member: usize,
}

impl ClusterDetailView<'_> {
    /// Places the cluster heading before members in the projection's existing order.
    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.heading_lines();
        lines.extend(
            self.detail
                .members
                .iter()
                .enumerate()
                .map(|(index, member)| self.member_line(index, member)),
        );
        lines
    }

    /// Explains generated identity, repository scope, and local inclusion counts.
    fn heading_lines(&self) -> Vec<Line<'static>> {
        let cluster = &self.detail.cluster;
        vec![
            Line::from(format!("Cluster #{} · {}", cluster.id, cluster.title)).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(format!(
                "{} · {} active · {} excluded{}",
                cluster.repository.full_name,
                cluster.active_member_count,
                cluster.excluded_member_count,
                if cluster.dismissed {
                    " · dismissed"
                } else {
                    ""
                }
            )),
            Line::from(""),
            Line::from("Members and neighbor scores")
                .style(Style::default().add_modifier(Modifier::BOLD)),
        ]
    }

    /// Shows one member's role, inclusion, optional score, and selection cue.
    fn member_line(&self, index: usize, member: &ClusterMember) -> Line<'static> {
        let state = member_state_name(member.state);
        let role = member_role_name(member.role);
        let selected = index == self.selected_member;
        let score = member
            .score_to_representative
            .map(|score| format!(" · score {score:.3}"))
            .unwrap_or_default();
        let discussion = &member.summary.discussion;
        let line = Line::from(format!(
            "{} #{} {} · {role} · {state}{score}",
            if selected { "›" } else { " " },
            discussion.id.number().get(),
            discussion.title
        ));
        if selected {
            line.style(selected_style())
        } else {
            line
        }
    }
}

/// Uses inclusion wording for active members, independently of their role in the cluster.
fn member_state_name(state: ClusterMemberState) -> &'static str {
    match state {
        ClusterMemberState::Active => "included",
        ClusterMemberState::Excluded => "excluded",
        ClusterMemberState::Removed => "removed",
    }
}

/// Labels explicit canonical choice separately from generated representative and related members.
fn member_role_name(role: ClusterMemberRole) -> &'static str {
    match role {
        ClusterMemberRole::Canonical => "canonical",
        ClusterMemberRole::Representative => "representative",
        ClusterMemberRole::Related => "related",
    }
}
