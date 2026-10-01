//! Cluster list and the selected cluster's members. Member roles, inclusion, and dismissal stay
//! visually distinct so a generated suggestion never reads as a maintainer decision.

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
use crate::view::{pane, selected_style};

pub fn draw_clusters(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let list = &mut app.cluster_list;
    let rows = &list.rows;
    let mut placeholder = ListState::default();
    let (items, state) = if rows.loading && rows.data.is_empty() {
        (vec![ListItem::new("Loading clusters…")], &mut placeholder)
    } else if let Some(error) = &rows.error {
        (vec![ListItem::new(error.clone())], &mut placeholder)
    } else if rows.data.is_empty() {
        let empty = "No generated clusters in this repository scope";
        (vec![ListItem::new(empty)], &mut placeholder)
    } else {
        (
            rows.data.iter().map(cluster_item).collect(),
            &mut list.state,
        )
    };
    let list = List::new(items)
        .block(pane("Generated clusters · neighbors", true))
        .highlight_style(selected_style());
    frame.render_stateful_widget(list, area, state);
}

pub fn draw_cluster_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let detail = &app.cluster_detail_pane.detail;
    let mut lines = if detail.loading && detail.data.is_none() {
        vec![Line::from("Loading cluster neighbors…")]
    } else if let Some(error) = &detail.error {
        vec![Line::from(error.clone())]
    } else if let Some(data) = &detail.data {
        let view = ClusterDetailView {
            detail: data,
            selected_member: app.cluster_detail_pane.members.selected(),
        };
        view.lines()
    } else {
        vec![Line::from(
            "Select a cluster to inspect its members and neighbor scores.",
        )]
    };
    if detail.loading && detail.data.is_some() {
        lines.insert(0, Line::from("Refreshing…"));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane("Cluster members and neighbors", true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

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

/// The member list is part of a wrapped paragraph, so selection is drawn as a marker and style.
struct ClusterDetailView<'a> {
    detail: &'a ClusterDetail,
    selected_member: Option<usize>,
}

impl ClusterDetailView<'_> {
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

    fn member_line(&self, index: usize, member: &ClusterMember) -> Line<'static> {
        let state = member_state_name(member.state);
        let role = member_role_name(member.role);
        let selected = Some(index) == self.selected_member;
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

fn member_state_name(state: ClusterMemberState) -> &'static str {
    match state {
        ClusterMemberState::Active => "included",
        ClusterMemberState::Excluded => "excluded",
        ClusterMemberState::Removed => "removed",
    }
}

fn member_role_name(role: ClusterMemberRole) -> &'static str {
    match role {
        ClusterMemberRole::Canonical => "canonical",
        ClusterMemberRole::Representative => "representative",
        ClusterMemberRole::Related => "related",
    }
}
