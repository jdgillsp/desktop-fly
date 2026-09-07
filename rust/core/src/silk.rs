//! Silk: the threads a spider lays, walks on, feels and cuts (WEB_PLAN.md §4).
//!
//! A [`Silk`] is a small graph. Nodes are points — either **fixed** to
//! something outside the web (a wall, a ledge, a prop, the floor) or **free**
//! junctions the spider made by attaching one thread to another. Threads join
//! two nodes and carry a [`ThreadKind`], which is what a construction program
//! reasons about ("follow the next auxiliary loop inward") and what the
//! renderer colours by.
//!
//! The one idea that makes this a construction model rather than a line list
//! is the **trailing line**: the spinnerets are always the free end of the
//! thread in progress. [`Silk::pay_out`] starts a line from a new node, the
//! spider walks, and [`Silk::attach`] closes it into a thread and starts the
//! next one from there. That two-call rhythm *is* web building, and it is
//! also the salticid's dragline: `pay_out` before a jump, [`Silk::release`]
//! on landing. Every spider in the app uses the same line.
//!
//! Vibration is a scalar excitation per node that spreads along threads and
//! decays. Prey and the cursor deposit it; a spider's legs read it. This is
//! not a physics model: tension is a scalar set when a thread is laid, and
//! nothing sags or blows.

use crate::util::{hypot, Vec2};

/// How fast excitation flows along a thread, per second per unit tension.
pub const SPREAD: f32 = 6.0;
/// Exponential decay rate of excitation, per second.
pub const DAMP: f32 = 3.0;
/// How far from a node an [`Silk::excite`] call still lands on it.
pub const EXCITE_REACH: f32 = 40.0;

/// What a node is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// Something outside the web: a wall, a ledge, a prop, the floor.
    Fixed,
    /// A junction the spider made by attaching one thread to another.
    Free,
}

/// The structural role of a thread. Which kinds exist is decided by the web
/// families in WEB_PLAN.md §5; a program only ever lays the kinds its family
/// builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadKind {
    /// The safety line every spider trails: attached before a jump, paid out
    /// on a descent.
    Dragline,
    /// The first thread across a gap, from which an orb hangs.
    Bridge,
    /// The outer polygon of an orb.
    Frame,
    /// Hub to frame, one out-and-back each.
    Radius,
    /// The wide, non-sticky scaffold spiral, laid outward and removed again.
    Auxiliary,
    /// The sticky capture spiral, laid inward.
    Capture,
    /// Three-dimensional mesh lines of a theridiid web.
    Tangle,
    /// A tensioned line down to the floor with a sticky foot.
    Gumfoot,
    /// A flat mesh, thickened by every crossing.
    Sheet,
    /// The dense silk of a retreat or funnel.
    Retreat,
}

impl ThreadKind {
    /// Whether prey sticks to it.
    pub fn is_sticky(self) -> bool {
        matches!(self, ThreadKind::Capture | ThreadKind::Gumfoot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    pub pos: Vec2,
    pub anchor: Anchor,
    /// Vibration energy at this node, decaying.
    pub excite: f32,
    /// For a fixed node, *what* it is fixed to: the id of a structure on the
    /// screen (`anchors::Frame`), so that when that structure moves or goes
    /// the threads on it can be cut and no others. `None` for a junction,
    /// and for a fixed end that reached nothing real.
    pub on: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thread {
    pub a: usize,
    pub b: usize,
    pub kind: ThreadKind,
    /// Length when laid; the current length may differ if a free node moved.
    pub rest_len: f32,
    /// 0..1-ish. Scales how fast excitation travels along it.
    pub tension: f32,
}

/// One drawable piece of silk. The trailing line is included as a segment
/// from its node to the spider so a renderer has nothing to special-case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub a: Vec2,
    pub b: Vec2,
    pub kind: ThreadKind,
    /// Mean excitation of the two ends, for a glow.
    pub excite: f32,
    /// True for the line in progress (its `b` end is the spider).
    pub trailing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Silk {
    pub nodes: Vec<Node>,
    pub threads: Vec<Thread>,
    /// The node the line in progress is paid out from, and what kind of
    /// thread it will be when attached. The other end is wherever the
    /// spider is.
    trailing: Option<(usize, ThreadKind)>,
}

impl Silk {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.threads.is_empty() && self.trailing.is_none()
    }

    /// Forget everything: a display change, a creature switch.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.threads.clear();
        self.trailing = None;
    }

    pub fn add_node(&mut self, pos: Vec2, anchor: Anchor) -> usize {
        self.nodes.push(Node {
            pos,
            anchor,
            excite: 0.0,
            on: None,
        });
        self.nodes.len() - 1
    }

    /// Start a line from a new fixed node at `at`. Any line already in
    /// progress is let go (its node stays only if a thread uses it).
    pub fn pay_out(&mut self, at: Vec2, kind: ThreadKind) -> usize {
        self.release();
        let n = self.add_node(at, Anchor::Fixed);
        self.trailing = Some((n, kind));
        n
    }

    /// Change what the line in progress will become when attached.
    pub fn set_kind(&mut self, kind: ThreadKind) {
        if let Some((n, _)) = self.trailing {
            self.trailing = Some((n, kind));
        }
    }

    /// Fix the line in progress to a new node at `at`, and carry on paying
    /// out from there. With no line in progress this just starts one.
    pub fn attach(&mut self, at: Vec2, anchor: Anchor) -> usize {
        let m = self.add_node(at, anchor);
        self.attach_to(m);
        m
    }

    /// Fix the line in progress to an existing node — a junction — and carry
    /// on from there.
    pub fn attach_to(&mut self, node: usize) {
        match self.trailing {
            Some((n, kind)) if n != node => {
                let len = dist(self.nodes[n].pos, self.nodes[node].pos);
                self.threads.push(Thread {
                    a: n,
                    b: node,
                    kind,
                    rest_len: len,
                    tension: 1.0,
                });
                self.trailing = Some((node, kind));
            }
            Some((_, kind)) => self.trailing = Some((node, kind)),
            None => self.trailing = Some((node, ThreadKind::Dragline)),
        }
    }

    /// Let go of the line in progress. A node that only ever held a released
    /// line is removed, so a retracted dragline leaves nothing behind.
    /// Returns where the line was anchored, if there was one.
    pub fn release(&mut self) -> Option<Vec2> {
        let (n, _) = self.trailing.take()?;
        let at = self.nodes[n].pos;
        if !self.threads.iter().any(|t| t.a == n || t.b == n) {
            self.remove_node(n);
        }
        Some(at)
    }

    /// Where the line in progress is anchored, if one is out.
    pub fn trailing_anchor(&self) -> Option<Vec2> {
        self.trailing.map(|(n, _)| self.nodes[n].pos)
    }

    pub fn trailing_kind(&self) -> Option<ThreadKind> {
        self.trailing.map(|(_, k)| k)
    }

    /// Sever every thread that passes within `r` of `p`, and drop any node
    /// that no longer holds anything. Returns how many threads went.
    pub fn cut_near(&mut self, p: Vec2, r: f32) -> usize {
        let before = self.threads.len();
        let nodes = &self.nodes;
        self.threads
            .retain(|t| segment_distance(nodes[t.a].pos, nodes[t.b].pos, p) >= r);
        let cut = before - self.threads.len();
        if cut > 0 {
            self.prune();
        }
        cut
    }

    /// Sever every thread with an end fixed on structure `id` — the window
    /// that moved or closed — and drop what no longer holds anything.
    /// Returns how many threads went.
    pub fn cut_on(&mut self, id: i64) -> usize {
        let before = self.threads.len();
        let nodes = &self.nodes;
        self.threads
            .retain(|t| nodes[t.a].on != Some(id) && nodes[t.b].on != Some(id));
        let cut = before - self.threads.len();
        if cut > 0 {
            self.prune();
        }
        cut
    }

    /// Deposit vibration into the silk at `at`: onto the nearest thread
    /// within reach, shared between its two ends by where along it the
    /// point lies (a struggle mid-thread shakes both ends). False if no silk
    /// is near.
    pub fn excite(&mut self, at: Vec2, amount: f32) -> bool {
        match self.thread_near(at, None, EXCITE_REACH) {
            Some((t, _)) => {
                let th = self.threads[t];
                let (a, b) = (self.nodes[th.a].pos, self.nodes[th.b].pos);
                let u = param_along(a, b, at);
                self.nodes[th.a].excite += amount * (1.0 - u);
                self.nodes[th.b].excite += amount * u;
                true
            }
            None => match self.nearest_node(at) {
                Some((i, d)) if d <= EXCITE_REACH => {
                    self.nodes[i].excite += amount;
                    true
                }
                _ => false,
            },
        }
    }

    /// The vibration a leg standing at `p` would feel: read off the nearest
    /// thread, interpolated between its ends.
    pub fn excitation_at(&self, p: Vec2) -> f32 {
        if let Some((i, d)) = self.nearest_node(p) {
            if d <= 2.0 {
                return self.nodes[i].excite;
            }
        }
        match self.thread_near(p, None, EXCITE_REACH) {
            Some((t, _)) => {
                let th = self.threads[t];
                let (a, b) = (self.nodes[th.a].pos, self.nodes[th.b].pos);
                let u = param_along(a, b, p);
                self.nodes[th.a].excite * (1.0 - u) + self.nodes[th.b].excite * u
            }
            None => match self.nearest_node(p) {
                Some((i, d)) if d <= EXCITE_REACH => self.nodes[i].excite,
                _ => 0.0,
            },
        }
    }

    pub fn nearest_node(&self, p: Vec2) -> Option<(usize, f32)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (i, dist(n.pos, p)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Spread excitation along threads and let it decay. Call at the sense
    /// rate; the constants are per second.
    pub fn step(&mut self, dt: f32) {
        if self.nodes.is_empty() {
            return;
        }
        let mut delta = vec![0.0f32; self.nodes.len()];
        for t in &self.threads {
            let flow = (self.nodes[t.a].excite - self.nodes[t.b].excite)
                * (SPREAD * t.tension * dt).min(0.5);
            delta[t.a] -= flow;
            delta[t.b] += flow;
        }
        let decay = (-DAMP * dt).exp();
        for (n, d) in self.nodes.iter_mut().zip(delta) {
            n.excite = ((n.excite + d) * decay).max(0.0);
        }
    }

    /// Everything drawable, the trailing line last.
    pub fn segments(&self, spider: Vec2) -> Vec<Segment> {
        let mut out: Vec<Segment> = self
            .threads
            .iter()
            .map(|t| Segment {
                a: self.nodes[t.a].pos,
                b: self.nodes[t.b].pos,
                kind: t.kind,
                excite: 0.5 * (self.nodes[t.a].excite + self.nodes[t.b].excite),
                trailing: false,
            })
            .collect();
        if let Some((n, kind)) = self.trailing {
            out.push(Segment {
                a: self.nodes[n].pos,
                b: spider,
                kind,
                excite: self.nodes[n].excite,
                trailing: true,
            });
        }
        out
    }

    /// Put a free node on thread `t` at the point of it nearest `at`, splitting
    /// the thread into two of the same kind. This is how a spiral is attached
    /// to a radius, or a radius to the frame: the new node is returned so the
    /// line in progress can be fixed to it.
    pub fn split_thread(&mut self, t: usize, at: Vec2) -> usize {
        let th = self.threads[t];
        let (a, b) = (self.nodes[th.a].pos, self.nodes[th.b].pos);
        let p = project_onto(a, b, at);
        let m = self.add_node(p, Anchor::Free);
        self.threads[t] = Thread {
            a: th.a,
            b: m,
            kind: th.kind,
            rest_len: dist(a, p),
            tension: th.tension,
        };
        self.threads.push(Thread {
            a: m,
            b: th.b,
            kind: th.kind,
            rest_len: dist(p, b),
            tension: th.tension,
        });
        m
    }

    /// Remove one thread and any node it leaves holding nothing. Thread and
    /// node indices are not stable across this call; look things up again.
    pub fn remove_thread(&mut self, t: usize) {
        if t < self.threads.len() {
            self.threads.remove(t);
            self.prune();
        }
    }

    /// The nearest thread to `p` within `r`, optionally of one kind.
    pub fn thread_near(&self, p: Vec2, kind: Option<ThreadKind>, r: f32) -> Option<(usize, f32)> {
        self.threads
            .iter()
            .enumerate()
            .filter(|(_, t)| kind.map(|k| t.kind == k).unwrap_or(true))
            .map(|(i, t)| (i, segment_distance(self.nodes[t.a].pos, self.nodes[t.b].pos, p)))
            .filter(|(_, d)| *d <= r)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// The point on thread `t` nearest `p`.
    pub fn point_on_thread(&self, t: usize, p: Vec2) -> Vec2 {
        let th = self.threads[t];
        project_onto(self.nodes[th.a].pos, self.nodes[th.b].pos, p)
    }

    /// A node within `r` of `p`, nearest first. Programs hold positions, not
    /// indices, because indices move when threads are cut; this is how they
    /// get an index back for one call.
    pub fn node_at(&self, p: Vec2, r: f32) -> Option<usize> {
        self.nearest_node(p).filter(|(_, d)| *d <= r).map(|(i, _)| i)
    }

    pub fn count_kind(&self, kind: ThreadKind) -> usize {
        self.threads.iter().filter(|t| t.kind == kind).count()
    }

    /// Total length of every thread of one kind.
    pub fn length_of_kind(&self, kind: ThreadKind) -> f32 {
        self.threads
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| dist(self.nodes[t.a].pos, self.nodes[t.b].pos))
            .sum()
    }

    /// Where the most excited node is, if any node is excited above `min`:
    /// the modelled localisation readout a sitting spider uses to decide
    /// which line to run down.
    pub fn loudest(&self, min: f32) -> Option<(Vec2, f32)> {
        self.nodes
            .iter()
            .filter(|n| n.excite > min)
            .max_by(|a, b| a.excite.partial_cmp(&b.excite).unwrap_or(std::cmp::Ordering::Equal))
            .map(|n| (n.pos, n.excite))
    }

    /// Every node's excitation summed: what the whole web is doing.
    pub fn total_excitation(&self) -> f32 {
        self.nodes.iter().map(|n| n.excite).sum()
    }

    /// Threads touching a node.
    pub fn degree(&self, node: usize) -> usize {
        self.threads.iter().filter(|t| t.a == node || t.b == node).count()
    }

    fn remove_node(&mut self, n: usize) {
        self.nodes.remove(n);
        for t in self.threads.iter_mut() {
            if t.a > n {
                t.a -= 1;
            }
            if t.b > n {
                t.b -= 1;
            }
        }
        if let Some((m, k)) = self.trailing {
            if m > n {
                self.trailing = Some((m - 1, k));
            }
        }
    }

    /// Drop nodes that hold no thread and are not the trailing anchor.
    fn prune(&mut self) {
        let mut i = 0;
        while i < self.nodes.len() {
            let held = self.threads.iter().any(|t| t.a == i || t.b == i)
                || self.trailing.map(|(n, _)| n == i).unwrap_or(false);
            if held {
                i += 1;
            } else {
                self.remove_node(i);
            }
        }
    }
}

fn dist(a: Vec2, b: Vec2) -> f32 {
    hypot(a.x - b.x, a.y - b.y)
}

/// Where along `ab` (0 at `a`, 1 at `b`) the point nearest `p` lies.
fn param_along(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let l2 = abx * abx + aby * aby;
    if l2 <= 1e-6 {
        0.0
    } else {
        (((p.x - a.x) * abx + (p.y - a.y) * aby) / l2).clamp(0.0, 1.0)
    }
}

/// The point on segment `ab` nearest `p`.
fn project_onto(a: Vec2, b: Vec2, p: Vec2) -> Vec2 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let l2 = abx * abx + aby * aby;
    let t = if l2 <= 1e-6 {
        0.0
    } else {
        (((p.x - a.x) * abx + (p.y - a.y) * aby) / l2).clamp(0.0, 1.0)
    };
    Vec2::new(a.x + abx * t, a.y + aby * t)
}

/// Distance from `p` to the segment `ab`.
fn segment_distance(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let l2 = abx * abx + aby * aby;
    let t = if l2 <= 1e-6 {
        0.0
    } else {
        (((p.x - a.x) * abx + (p.y - a.y) * aby) / l2).clamp(0.0, 1.0)
    };
    hypot(p.x - (a.x + abx * t), p.y - (a.y + aby * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y)
    }

    #[test]
    fn a_released_dragline_leaves_nothing_behind() {
        // The salticid's line: attached before a jump, retracted on landing.
        let mut s = Silk::new();
        s.pay_out(v(10.0, 20.0), ThreadKind::Dragline);
        assert_eq!(s.trailing_anchor(), Some(v(10.0, 20.0)));
        assert_eq!(s.segments(v(50.0, 20.0)).len(), 1);
        assert!(s.segments(v(50.0, 20.0))[0].trailing);
        assert_eq!(s.release(), Some(v(10.0, 20.0)));
        assert!(s.is_empty());
        assert!(s.nodes.is_empty(), "a node that only held a released line goes");
        assert_eq!(s.release(), None);
    }

    #[test]
    fn walking_and_attaching_lays_a_chain_of_threads() {
        let mut s = Silk::new();
        s.pay_out(v(0.0, 0.0), ThreadKind::Frame);
        let b = s.attach(v(100.0, 0.0), Anchor::Fixed);
        s.attach(v(100.0, 80.0), Anchor::Fixed);
        // Close the triangle back onto an existing node: a junction.
        s.set_kind(ThreadKind::Radius);
        s.attach_to(0);
        assert_eq!(s.threads.len(), 3);
        assert_eq!(s.threads[0].kind, ThreadKind::Frame);
        assert_eq!(s.threads[2].kind, ThreadKind::Radius);
        assert!((s.threads[0].rest_len - 100.0).abs() < 1e-4);
        assert_eq!(s.degree(b), 2);
        // Still paying out from the node just attached to.
        assert_eq!(s.trailing_anchor(), Some(v(0.0, 0.0)));
        let segs = s.segments(v(30.0, 30.0));
        assert_eq!(segs.len(), 4);
        assert!(segs[3].trailing);
        s.release();
        assert_eq!(s.nodes.len(), 3, "nodes that hold threads stay");
    }

    #[test]
    fn a_cut_removes_only_the_threads_it_crosses_and_keeps_indices_sane() {
        let mut s = Silk::new();
        s.pay_out(v(0.0, 0.0), ThreadKind::Radius);
        s.attach(v(100.0, 0.0), Anchor::Fixed); // thread 0: (0,0)-(100,0)
        s.pay_out(v(0.0, 50.0), ThreadKind::Radius);
        s.attach(v(100.0, 50.0), Anchor::Fixed); // thread 1: (0,50)-(100,50)
        s.pay_out(v(200.0, 200.0), ThreadKind::Dragline);
        assert_eq!(s.nodes.len(), 5);
        // A sweep through the lower thread only.
        let cut = s.cut_near(v(50.0, 3.0), 5.0);
        assert_eq!(cut, 1);
        assert_eq!(s.threads.len(), 1);
        let t = s.threads[0];
        assert_eq!(s.nodes[t.a].pos, v(0.0, 50.0));
        assert_eq!(s.nodes[t.b].pos, v(100.0, 50.0));
        // The orphaned ends went; the trailing anchor survived and re-indexed.
        assert_eq!(s.nodes.len(), 3);
        assert_eq!(s.trailing_anchor(), Some(v(200.0, 200.0)));
        assert_eq!(s.cut_near(v(500.0, 500.0), 5.0), 0);
    }

    #[test]
    fn a_pulse_travels_down_a_chain_and_dies_away() {
        // A radius: five nodes in a line, excite one end, watch the other.
        let mut s = Silk::new();
        s.pay_out(v(0.0, 0.0), ThreadKind::Radius);
        for i in 1..5 {
            s.attach(v(i as f32 * 30.0, 0.0), Anchor::Free);
        }
        s.release();
        assert!(s.excite(v(1.0, 0.0), 1.0));
        assert!(!s.excite(v(0.0, 500.0), 1.0), "out of reach lands nowhere");
        let far = v(120.0, 0.0);
        assert_eq!(s.excitation_at(far), 0.0);
        let mut peak = 0.0f32;
        for _ in 0..30 {
            s.step(1.0 / 30.0);
            peak = peak.max(s.excitation_at(far));
        }
        assert!(peak > 0.02, "the pulse reached the far end: peak {peak}");
        for _ in 0..300 {
            s.step(1.0 / 30.0);
        }
        assert!(s.excitation_at(far) < 1e-3, "and decayed: {}", s.excitation_at(far));
        assert!(s.excitation_at(v(0.0, 0.0)) < 1e-3);
    }

    #[test]
    fn paying_out_again_lets_the_old_line_go() {
        let mut s = Silk::new();
        s.pay_out(v(0.0, 0.0), ThreadKind::Dragline);
        s.pay_out(v(9.0, 9.0), ThreadKind::Dragline);
        assert_eq!(s.nodes.len(), 1);
        assert_eq!(s.trailing_anchor(), Some(v(9.0, 9.0)));
        assert_eq!(s.trailing_kind(), Some(ThreadKind::Dragline));
    }

    #[test]
    fn splitting_a_thread_makes_a_junction_of_the_same_kind() {
        let mut s = Silk::new();
        s.pay_out(v(0.0, 0.0), ThreadKind::Radius);
        s.attach(v(100.0, 0.0), Anchor::Fixed);
        s.release();
        // A spiral arriving at (40, 7) fixes to the radius at (40, 0).
        let (t, _) = s.thread_near(v(40.0, 7.0), Some(ThreadKind::Radius), 10.0).unwrap();
        let m = s.split_thread(t, v(40.0, 7.0));
        assert_eq!(s.nodes[m].pos, v(40.0, 0.0));
        assert_eq!(s.nodes[m].anchor, Anchor::Free);
        assert_eq!(s.count_kind(ThreadKind::Radius), 2);
        assert!((s.length_of_kind(ThreadKind::Radius) - 100.0).abs() < 1e-3);
        assert_eq!(s.degree(m), 2);
        assert_eq!(s.node_at(v(41.0, 1.0), 3.0), Some(m));
        assert!(s.thread_near(v(40.0, 30.0), Some(ThreadKind::Capture), 10.0).is_none());
        s.remove_thread(0);
        assert_eq!(s.threads.len(), 1);
        assert_eq!(s.nodes.len(), 2, "the orphaned end went");
    }

    #[test]
    fn the_loudest_node_is_where_the_struggle_is() {
        let mut s = Silk::new();
        s.pay_out(v(0.0, 0.0), ThreadKind::Radius);
        s.attach(v(100.0, 0.0), Anchor::Fixed);
        s.attach(v(200.0, 0.0), Anchor::Fixed);
        s.release();
        assert!(s.loudest(0.01).is_none());
        s.excite(v(198.0, 2.0), 0.5);
        assert_eq!(s.loudest(0.01).map(|(p, _)| p), Some(v(200.0, 0.0)));
    }

    #[test]
    fn sticky_kinds_are_the_capture_kinds() {
        assert!(ThreadKind::Capture.is_sticky());
        assert!(ThreadKind::Gumfoot.is_sticky());
        assert!(!ThreadKind::Radius.is_sticky());
        assert!(!ThreadKind::Sheet.is_sticky());
        assert!(!ThreadKind::Dragline.is_sticky());
    }
}
