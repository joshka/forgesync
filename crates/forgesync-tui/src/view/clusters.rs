//! Clusters screen rendering.

use super::{
    App, ClusterDetail, ClusterLifecycle, ClusterMemberRole, ClusterMemberState, Color, Frame,
    Line, List, ListItem, ListState, Modifier, Paragraph, Rect, Style, Text, Wrap, pane_block,
    selected_style,
};

pub fn draw_clusters(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut items: Vec<ListItem<'_>> = app
        .clusters
        .iter()
        .map(|cluster| {
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
                cluster.title,
                lifecycle,
                cluster.active_member_count,
                cluster.excluded_member_count
            ))
        })
        .collect();
    if app.clusters_loading && items.is_empty() {
        items.push(ListItem::new("Loading clusters…"));
    } else if let Some(error) = &app.clusters_error {
        items = vec![ListItem::new(error.clone())];
    } else if items.is_empty() {
        items.push(ListItem::new(
            "No generated clusters in this repository scope",
        ));
    }
    let mut state = ListState::default();
    if app.clusters_error.is_none() && !app.clusters.is_empty() {
        state.select(Some(app.selected_cluster));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(pane_block("Generated clusters · neighbors", true))
            .highlight_style(selected_style()),
        area,
        &mut state,
    );
}

pub fn draw_cluster_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = if app.cluster_detail_loading && app.cluster_detail.is_none() {
        vec![Line::from("Loading cluster neighbors…")]
    } else if let Some(error) = &app.cluster_detail_error {
        vec![Line::from(error.clone())]
    } else if let Some(detail) = &app.cluster_detail {
        cluster_detail_lines(detail, app.selected_cluster_member)
    } else {
        vec![Line::from(
            "Select a cluster to inspect its members and neighbor scores.",
        )]
    };
    if app.cluster_detail_loading && app.cluster_detail.is_some() {
        lines.insert(0, Line::from("Refreshing…"));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block("Cluster members and neighbors", true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn cluster_detail_lines(detail: &ClusterDetail, selected_member: usize) -> Vec<Line<'static>> {
    let cluster = &detail.cluster;
    let mut lines = vec![
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
    ];
    lines.extend(detail.members.iter().enumerate().map(|(index, member)| {
        let state = match member.state {
            ClusterMemberState::Active => "included",
            ClusterMemberState::Excluded => "excluded",
            ClusterMemberState::Removed => "removed",
        };
        let role = match member.role {
            ClusterMemberRole::Canonical => "canonical",
            ClusterMemberRole::Representative => "representative",
            ClusterMemberRole::Related => "related",
        };
        let score = member
            .score_to_representative
            .map(|score| format!(" · score {score:.3}"))
            .unwrap_or_default();
        let discussion = &member.summary.discussion;
        let line = Line::from(format!(
            "{} #{} {} · {role} · {state}{score}",
            if index == selected_member { "›" } else { " " },
            discussion.id.number().get(),
            discussion.title
        ));
        if index == selected_member {
            line.style(selected_style())
        } else {
            line
        }
    }));
    lines
}
