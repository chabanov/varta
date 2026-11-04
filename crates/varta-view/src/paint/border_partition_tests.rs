use super::*;
use crate::{Environment, units::Units};

fn bounds() -> BoxRect {
    BoxRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    }
}
fn radii(raw: &str) -> Corners {
    Corners::resolve(raw, bounds(), Units::new(&Environment::default()).unwrap()).unwrap()
}

#[test]
fn oversized_corner_keeps_zero_width_ownership_instead_of_whole_ring_nearest_edge() {
    let p = Partition::new(bounds(), [30.0, 0.0, 30.0, 0.0], radii("90px 0 0 0"));
    assert_eq!(p.owner(10.5, 60.5), TOP);
    assert_eq!(p.owner(10.5, 80.5), BOTTOM);
    for y in 0..100 {
        for x in 0..100 {
            assert!(matches!(
                p.owner(f64::from(x) + 0.5, f64::from(y) + 0.5),
                TOP | BOTTOM
            ));
        }
    }
}

#[test]
fn square_corners_follow_width_ratios_and_zero_width_sides_never_own_area() {
    for widths in [
        [10.0, 20.0, 30.0, 5.0],
        [0.0, 20.0, 30.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
    ] {
        let p = Partition::new(bounds(), widths, Corners::default());
        for y in 0..100 {
            for x in 0..100 {
                let point = [f64::from(x) + 0.37, f64::from(y) + 0.43];
                let distances = [point[1], 100.0 - point[0], 100.0 - point[1], point[0]];
                let expected = (0..SIDES)
                    .filter(|i| widths[*i] > 0.0)
                    .min_by(|a, b| {
                        (distances[*a] / f64::from(widths[*a]))
                            .total_cmp(&(distances[*b] / f64::from(widths[*b])))
                    })
                    .unwrap();
                assert_eq!(p.owner(point[0], point[1]), expected);
            }
        }
    }
}

#[test]
fn rotation_preserves_curved_ownership_including_the_oversized_inset_counterexample() {
    let mut widths = [30.0, 0.0, 30.0, 0.0];
    let mut corners = radii("90px 0 0 0");
    let samples = [
        [10.5, 60.5],
        [10.5, 80.5],
        [92.3, 3.7],
        [3.7, 92.3],
        [20.3, 40.7],
    ];
    let reference = Partition::new(bounds(), widths, corners);
    for turn in 0..SIDES {
        let rotated = Partition::new(bounds(), widths, corners);
        for point in samples {
            let mut q = point;
            for _ in 0..turn {
                q = [100.0 - q[1], q[0]];
            }
            assert_eq!(
                rotated.owner(q[0], q[1]),
                (reference.owner(point[0], point[1]) + turn) % SIDES
            );
        }
        widths.rotate_right(1);
        corners.0.rotate_right(1);
        for r in &mut corners.0 {
            r.swap(0, 1);
        }
    }
}

// Independent reference: binary-search the existing padding-edge membership,
// then identify the one constraint that excludes the point just beyond it.
// Production uses analytic sign tests for ordinary single-corner ownership.
fn oracle(widths: [f32; 4], corners: Corners, point: [f32; 2]) -> Option<usize> {
    let at = |t: f32| corners.inset(bounds(), widths.map(|w| w * t));
    let (hole, inner) = at(1.0);
    if !corners.contains(bounds(), point[0], point[1]) || inner.contains(hole, point[0], point[1]) {
        return None;
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..32 {
        let t = (low + high) * 0.5;
        let (b, r) = at(t);
        if r.contains(b, point[0], point[1]) {
            low = t;
        } else {
            high = t;
        }
    }
    let (b, r) = at((high + 0.00001).min(1.0));
    let mut owners = Vec::new();
    for (side, violates) in [
        point[1] < b.y,
        point[0] > b.x + b.width,
        point[1] > b.y + b.height,
        point[0] < b.x,
    ]
    .into_iter()
    .enumerate()
    {
        if violates {
            owners.push(side);
        }
    }
    let corner_edges = [
        [b.x, b.y],
        [b.x + b.width, b.y],
        [b.x + b.width, b.y + b.height],
        [b.x, b.y + b.height],
    ];
    for (index, (edge, [rx, ry])) in corner_edges.into_iter().zip(r.0).enumerate() {
        if rx == 0.0 || ry == 0.0 {
            continue;
        }
        let horizontal = if index < 2 { TOP } else { BOTTOM };
        let vertical = if index == 0 || index == 3 {
            LEFT
        } else {
            RIGHT
        };
        let dx = (point[0] - edge[0]) * if vertical == LEFT { 1.0 } else { -1.0 };
        let dy = (point[1] - edge[1]) * if horizontal == TOP { 1.0 } else { -1.0 };
        if dx < rx && dy < ry && dx >= 0.0 && dy >= 0.0 {
            let u = f64::from((rx - dx) / rx);
            let v = f64::from((ry - dy) / ry);
            if u * u + v * v > 1.0 {
                let ratio = f64::from(widths[horizontal])
                    / f64::from(widths[horizontal] + widths[vertical]);
                owners.push(if u.atan2(v) <= std::f64::consts::FRAC_PI_2 * ratio {
                    horizontal
                } else {
                    vertical
                });
            }
        }
    }
    (owners.len() == 1).then(|| owners[0])
}

#[test]
fn analytic_partition_matches_independent_inset_membership_for_asymmetric_and_colliding_corners() {
    let mut state = 771_u32;
    let mut next = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        state
    };
    let mut checked = 0;
    for _ in 0..32 {
        let raw = format!(
            "{}px {}px {}px {}px",
            next() % 180,
            next() % 180,
            next() % 180,
            next() % 180
        );
        let corners = radii(&raw);
        let widths = std::array::from_fn(|_| (next() % 40) as f32);
        if widths == [0.0; 4] {
            continue;
        }
        let p = Partition::new(bounds(), widths, corners);
        for _ in 0..256 {
            let point = [
                (next() % 10000) as f32 / 100.0,
                (next() % 10000) as f32 / 100.0,
            ];
            if let Some(expected) = oracle(widths, corners, point) {
                assert_eq!(
                    p.owner(f64::from(point[0]), f64::from(point[1])),
                    expected,
                    "{raw}, {widths:?}, {point:?}"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 2000,
        "reference must exercise actual border points: {checked}"
    );
}

#[test]
fn uniform_blocks_never_assign_a_color_that_conflicts_with_point_ownership() {
    let mut proven = 0;
    for raw in ["0", "20px", "90px 0 90px 0", "0 100% 0 0", "50%"] {
        for widths in [[8.0; 4], [30.0, 0.0, 30.0, 0.0], [1.0, 20.0, 40.0, 3.0]] {
            let p = Partition::new(bounds(), widths, radii(raw));
            for y in (0..100).step_by(4) {
                for x in (0..100).step_by(4) {
                    if let Some(side) = p.uniform(f64::from(x), f64::from(y), 4.0, 4.0) {
                        proven += 1;
                        for dy in 0..8 {
                            for dx in 0..8 {
                                let point = [
                                    f64::from(x) + f64::from(dx) * 0.5 + 0.25,
                                    f64::from(y) + f64::from(dy) * 0.5 + 0.25,
                                ];
                                assert_eq!(
                                    p.owner(point[0], point[1]),
                                    side,
                                    "{raw}, {widths:?}, block{x}/{y}, point{point:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        proven > 1000,
        "actual uniform blocks must be exercised: {proven}"
    );
}
