use std::collections::BTreeMap;

use crate::egui::{Pos2, Rect, Vec2};
use layout::core::style::StyleAttr;

use super::KanbanId;

/// Returns the point on the boundary of `rect` where the line from `from_center`
/// toward `to_center` exits the rect.
pub fn rect_edge_point(from_center: Pos2, to_center: Pos2, rect: Rect) -> Pos2 {
    let dir = to_center - from_center;
    if dir.length_sq() < 0.1 {
        return from_center;
    }
    let mut t_best = f32::INFINITY;
    if dir.x.abs() > 0.001 {
        for &edge_x in &[rect.min.x, rect.max.x] {
            let t = (edge_x - from_center.x) / dir.x;
            if t > 0.0 {
                let y = from_center.y + t * dir.y;
                if y >= rect.min.y && y <= rect.max.y && t < t_best {
                    t_best = t;
                }
            }
        }
    }
    if dir.y.abs() > 0.001 {
        for &edge_y in &[rect.min.y, rect.max.y] {
            let t = (edge_y - from_center.y) / dir.y;
            if t > 0.0 {
                let x = from_center.x + t * dir.x;
                if x >= rect.min.x && x <= rect.max.x && t < t_best {
                    t_best = t;
                }
            }
        }
    }
    if t_best.is_finite() {
        from_center + dir * t_best
    } else {
        from_center
    }
}

pub fn estimate_node_size(wrapped_text: &str, font_size: f32) -> Vec2 {
    let char_width = font_size * 0.55;
    let line_height = font_size * 1.5;
    let lines: Vec<&str> = wrapped_text.lines().collect();
    let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1);
    Vec2::new(
        max_line_len as f32 * char_width + 20.0,
        lines.len().max(1) as f32 * line_height + 16.0,
    )
}

//  Barnes-Hut quadtree

enum QTContent {
    Empty,
    Leaf(Vec2, f32), // position, half-extent radius
    Internal(Box<[Option<QuadTree>; 4]>),
}

struct QuadTree {
    bounds_min: Vec2,
    bounds_max: Vec2,
    /// Aggregate center of mass of all particles in this cell
    com: Vec2,
    /// Total mass of all particles in this cell
    mass: f32,
    content: QTContent,
}

impl QuadTree {
    fn new(bounds_min: Vec2, bounds_max: Vec2) -> Self {
        QuadTree {
            bounds_min,
            bounds_max,
            com: Vec2::ZERO,
            mass: 0.0,
            content: QTContent::Empty,
        }
    }

    fn quadrant_index(&self, pos: Vec2) -> usize {
        let mid_x = (self.bounds_min.x + self.bounds_max.x) * 0.5;
        let mid_y = (self.bounds_min.y + self.bounds_max.y) * 0.5;
        (pos.x >= mid_x) as usize | (((pos.y >= mid_y) as usize) << 1)
    }

    fn child_bounds(&self, q: usize) -> (Vec2, Vec2) {
        let mid_x = (self.bounds_min.x + self.bounds_max.x) * 0.5;
        let mid_y = (self.bounds_min.y + self.bounds_max.y) * 0.5;
        (
            Vec2::new(
                if q & 1 == 0 { self.bounds_min.x } else { mid_x },
                if q & 2 == 0 { self.bounds_min.y } else { mid_y },
            ),
            Vec2::new(
                if q & 1 == 0 { mid_x } else { self.bounds_max.x },
                if q & 2 == 0 { mid_y } else { self.bounds_max.y },
            ),
        )
    }

    fn insert(&mut self, pos: Vec2, mass: f32, radius: f32) {
        // Update aggregate center of mass before descending
        let new_total = self.mass + mass;
        self.com = (self.com * self.mass + pos * mass) / new_total;
        self.mass = new_total;

        match std::mem::replace(&mut self.content, QTContent::Empty) {
            QTContent::Empty => {
                self.content = QTContent::Leaf(pos, radius);
            }
            QTContent::Leaf(old_pos, old_radius) => {
                let diff = old_pos - pos;
                if diff.length_sq() < 0.001 {
                    // Coincident particles: keep single leaf, masses already merged above
                    self.content = QTContent::Leaf(old_pos, old_radius);
                    return;
                }
                let old_mass = new_total - mass;
                let mut children: Box<[Option<QuadTree>; 4]> = Box::new([None, None, None, None]);
                let q = self.quadrant_index(old_pos);
                let (mn, mx) = self.child_bounds(q);
                children[q] = Some(QuadTree::new(mn, mx));
                children[q]
                    .as_mut()
                    .unwrap()
                    .insert(old_pos, old_mass, old_radius);
                let q = self.quadrant_index(pos);
                let (mn, mx) = self.child_bounds(q);
                if children[q].is_none() {
                    children[q] = Some(QuadTree::new(mn, mx));
                }
                children[q].as_mut().unwrap().insert(pos, mass, radius);
                self.content = QTContent::Internal(children);
            }
            QTContent::Internal(mut children) => {
                let q = self.quadrant_index(pos);
                if children[q].is_none() {
                    let (mn, mx) = self.child_bounds(q);
                    children[q] = Some(QuadTree::new(mn, mx));
                }
                children[q].as_mut().unwrap().insert(pos, mass, radius);
                self.content = QTContent::Internal(children);
            }
        }
    }

    /// Returns the repulsive force on a particle at `pos` from all particles
    /// in this cell, using the Barnes-Hut theta criterion.
    ///
    /// `own_radius` is the half-extent of the querying node. For leaf
    /// interactions the force magnitude is computed on the gap between node
    /// edges rather than center-to-center distance, preventing overlap.
    fn repulsion_force(&self, pos: Vec2, own_radius: f32, k: f32, theta: f32) -> Vec2 {
        if self.mass == 0.0 {
            return Vec2::ZERO;
        }
        let d = pos - self.com;
        let dist2 = d.length_sq();
        // Skip self-interaction
        if dist2 < 1.0 {
            return Vec2::ZERO;
        }
        let dist = dist2.sqrt();
        let use_aggregate = match &self.content {
            QTContent::Empty => return Vec2::ZERO,
            QTContent::Leaf(_, _) => true,
            QTContent::Internal(_) => {
                let cell_size = (self.bounds_max.x - self.bounds_min.x)
                    .max(self.bounds_max.y - self.bounds_min.y);
                cell_size / dist < theta
            }
        };
        if use_aggregate {
            // For a leaf, use the gap between node edges so the force acts on
            // empty space rather than center distance. Floor at a small value
            // so overlapping nodes still get a finite (strong) push apart.
            let effective_dist = if let QTContent::Leaf(_, leaf_radius) = &self.content {
                (dist - leaf_radius - own_radius).max(k * 0.05)
            } else {
                dist
            };
            let f = k * k * self.mass / (effective_dist * effective_dist);
            d / dist * f
        } else if let QTContent::Internal(children) = &self.content {
            children
                .iter()
                .flatten()
                .map(|child| child.repulsion_force(pos, own_radius, k, theta))
                .fold(Vec2::ZERO, |acc, v| acc + v)
        } else {
            unreachable!()
        }
    }
}

// Layout algorithm

/// Force-directed layout based on ForceAtlas2 (Jacomy et al. 2014).
///
/// Differences from plain Fruchterman-Reingold:
/// - Barnes-Hut quadtree for O(n log n) repulsion instead of O(n²)
/// - Degree-weighted node masses so hubs push neighbours further out,
///   producing cleaner cluster separation (ForceAtlas2's defining feature)
/// - Gap-corrected repulsion: force magnitude uses the empty space between
///   node edges rather than center-to-center distance
pub fn force_atlas2(
    node_data: &[(KanbanId, String, StyleAttr, Vec2)],
    edges: &[(KanbanId, KanbanId)],
    iterations: u32,
    stable_positions: &BTreeMap<KanbanId, Pos2>,
) -> BTreeMap<KanbanId, Pos2> {
    let n = node_data.len();
    if n == 0 {
        return BTreeMap::new();
    }

    // k is the ideal spring length; base it on average node size so nodes
    // naturally end up non-overlapping without a per-pair size correction.
    let avg_size = node_data
        .iter()
        .map(|(_, _, _, sz)| sz.x.max(sz.y))
        .sum::<f32>()
        / n as f32;
    let k = (avg_size * 1.5).max(150.0);
    let t0 = k * 2.0;
    let cooling = t0 / iterations as f32;
    const THETA: f32 = 0.5;

    // Degree of each node used as its "mass" — hubs repel more strongly,
    // producing the cluster separation ForceAtlas2 is known for.
    let mut degree: BTreeMap<KanbanId, u32> =
        node_data.iter().map(|(id, _, _, _)| (*id, 0u32)).collect();
    for &(src, dst) in edges {
        if let Some(d) = degree.get_mut(&src) {
            *d += 1;
        }
        if let Some(d) = degree.get_mut(&dst) {
            *d += 1;
        }
    }

    let centroid = if stable_positions.is_empty() {
        Pos2::ZERO
    } else {
        let sum = stable_positions
            .values()
            .fold(Vec2::ZERO, |acc, p: &Pos2| acc + p.to_vec2());
        (sum / stable_positions.len() as f32).to_pos2()
    };

    let mut positions: BTreeMap<KanbanId, Vec2> = node_data
        .iter()
        .enumerate()
        .map(|(i, (id, _, _, _))| {
            if let Some(p) = stable_positions.get(id) {
                (*id, p.to_vec2())
            } else {
                let angle = 2.0 * std::f32::consts::PI * i as f32 / n as f32;
                let r = k * (n as f32).sqrt().max(1.0);
                (*id, Vec2::new(centroid.x + r * angle.cos(), centroid.y + r * angle.sin()))
            }
        })
        .collect();

    let ids: Vec<KanbanId> = node_data.iter().map(|(id, _, _, _)| *id).collect();
    // Half-extent radius: longest dimension / 2.  Used for gap-based repulsion
    // so that the force acts on empty space between node edges, not centers.
    let radius_map: BTreeMap<KanbanId, f32> = node_data
        .iter()
        .map(|(id, _, _, sz)| (*id, sz.x.max(sz.y) * 0.5))
        .collect();
    let mut temp = t0;

    for _ in 0..iterations {
        let mut disp: BTreeMap<KanbanId, Vec2> =
            ids.iter().map(|&id| (id, Vec2::ZERO)).collect();

        // Build quadtree for O(n log n) repulsion
        let min_x = positions.values().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let min_y = positions.values().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_x = positions.values().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let max_y = positions.values().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        let margin = k;
        let mut tree = QuadTree::new(
            Vec2::new(min_x - margin, min_y - margin),
            Vec2::new(max_x + margin, max_y + margin),
        );
        for (&id, &pos) in &positions {
            tree.insert(pos, (degree[&id] + 1) as f32, radius_map[&id]);
        }

        // Repulsion — Barnes-Hut, degree-weighted, gap-corrected
        for &id in &ids {
            let pos = positions[&id];
            let own_mass = (degree[&id] + 1) as f32;
            let own_radius = radius_map[&id];
            let f = tree.repulsion_force(pos, own_radius, k, THETA);
            *disp.get_mut(&id).unwrap() += f * own_mass;
        }

        // Attraction — O(m), unchanged
        for &(src, dst) in edges {
            if !positions.contains_key(&src) || !positions.contains_key(&dst) {
                continue;
            }
            let pi = positions[&src];
            let pj = positions[&dst];
            let d = pi - pj;
            let dist = d.length();
            if dist < 1.0 {
                continue;
            }
            let f = d * (dist / k);
            *disp.get_mut(&src).unwrap() -= f;
            *disp.get_mut(&dst).unwrap() += f;
        }

        // Apply displacements, clamped to temperature
        for &id in &ids {
            let d = disp[&id];
            let dist = d.length();
            if dist < 1.0 {
                continue;
            }
            let scale = dist.min(temp) / dist;
            *positions.get_mut(&id).unwrap() += d * scale;
        }

        temp = (temp - cooling).max(0.0);
    }

    positions
        .into_iter()
        .map(|(id, v)| (id, v.to_pos2()))
        .collect()
}
