//! A small behavior-tree engine (pure Rust, no Bevy), used by every NPC.
//!
//! A tree is built from [`Node`]s: [`Node::Sequence`] and [`Node::Selector`] combine children,
//! [`Node::Not`] and [`Node::Always`] adjust a child's answer, and [`Node::Leaf`] hands a
//! game-specific leaf (a condition or an action) to a [`Host`], which answers with a [`Status`].
//! The engine knows nothing about NPCs, tiles, or time, so it is tested with plain fake hosts
//! and the NPC rules (see `npc.rs`) plug in their own leaves.
//!
//! Rules:
//!
//! - **Reactive ticking.** Every tick starts at the root and re-evaluates from the left, so a
//!   condition placed before an action is checked again every tick. That is what lets an NPC
//!   drop what it is doing the moment the player talks to it. A node that must remember
//!   progress (walking a route, a countdown) keeps that state in the host, not in the tree.
//! - [`Node::Sequence`] runs its children in order and stops at the first child that is not
//!   `Success` (returning that status). All `Success` gives `Success`.
//! - [`Node::Selector`] runs its children in order and stops at the first child that is not
//!   `Failure` (returning that status). All `Failure` gives `Failure`.
//! - [`Node::Not`] swaps `Success` and `Failure`; `Running` stays `Running`.
//! - [`Node::Always`] turns `Failure` into `Success`; the other answers stay as they are.
//! - Trees are checked once by [`Node::validate`]: a sequence or selector must have a child
//!   and a tree may not be deeper than [`MAX_DEPTH`], so a bad tree is refused when it is
//!   built and never misbehaves while the game runs.

use std::fmt;

/// The most levels a tree may nest (the root counts as level 1).
pub const MAX_DEPTH: usize = 16;

/// What a node reports after one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Finished, and it worked (or the condition holds).
    Success,
    /// Finished, and it did not work (or the condition does not hold).
    Failure,
    /// Still going; tick again next time.
    Running,
}

/// A node of a behavior tree whose leaves are of type `L`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node<L> {
    Sequence(Vec<Node<L>>),
    Selector(Vec<Node<L>>),
    Not(Box<Node<L>>),
    Always(Box<Node<L>>),
    Leaf(L),
}

/// Whatever runs the leaves: for NPCs, a view of one NPC's state and senses.
pub trait Host<L> {
    fn run(&mut self, leaf: &L) -> Status;
}

/// Why a tree was refused by [`Node::validate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeError {
    /// A sequence or selector with no children.
    EmptyComposite,
    /// More than [`MAX_DEPTH`] levels.
    TooDeep,
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyComposite => write!(f, "a sequence or selector has no children"),
            Self::TooDeep => write!(f, "the tree is nested more than {MAX_DEPTH} levels deep"),
        }
    }
}

impl<L> Node<L> {
    pub fn leaf(leaf: L) -> Self {
        Self::Leaf(leaf)
    }

    pub fn sequence(children: Vec<Node<L>>) -> Self {
        Self::Sequence(children)
    }

    pub fn selector(children: Vec<Node<L>>) -> Self {
        Self::Selector(children)
    }

    pub fn not(child: Node<L>) -> Self {
        Self::Not(Box::new(child))
    }

    pub fn always(child: Node<L>) -> Self {
        Self::Always(Box::new(child))
    }

    /// Runs the tree once from this node.
    pub fn tick(&self, host: &mut impl Host<L>) -> Status {
        match self {
            Self::Leaf(leaf) => host.run(leaf),
            Self::Sequence(children) => {
                for child in children {
                    match child.tick(host) {
                        Status::Success => {}
                        other => return other,
                    }
                }
                Status::Success
            }
            Self::Selector(children) => {
                for child in children {
                    match child.tick(host) {
                        Status::Failure => {}
                        other => return other,
                    }
                }
                Status::Failure
            }
            Self::Not(child) => match child.tick(host) {
                Status::Success => Status::Failure,
                Status::Failure => Status::Success,
                Status::Running => Status::Running,
            },
            Self::Always(child) => match child.tick(host) {
                Status::Failure => Status::Success,
                other => other,
            },
        }
    }

    /// Checks the whole tree (see the module rules).
    pub fn validate(&self) -> Result<(), TreeError> {
        self.validate_at(1)
    }

    fn validate_at(&self, depth: usize) -> Result<(), TreeError> {
        if depth > MAX_DEPTH {
            return Err(TreeError::TooDeep);
        }
        match self {
            Self::Leaf(_) => Ok(()),
            Self::Sequence(children) | Self::Selector(children) => {
                if children.is_empty() {
                    return Err(TreeError::EmptyComposite);
                }
                children.iter().try_for_each(|child| child.validate_at(depth + 1))
            }
            Self::Not(child) | Self::Always(child) => child.validate_at(depth + 1),
        }
    }

    /// How many leaves the tree has.
    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Leaf(_) => 1,
            Self::Sequence(children) | Self::Selector(children) => children.iter().map(Node::leaf_count).sum(),
            Self::Not(child) | Self::Always(child) => child.leaf_count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake host: each leaf is a fixed status, and every run is recorded.
    struct Script {
        ran: Vec<&'static str>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Fixed(&'static str, Status);

    impl Host<Fixed> for Script {
        fn run(&mut self, leaf: &Fixed) -> Status {
            self.ran.push(leaf.0);
            leaf.1
        }
    }

    fn leaf(name: &'static str, status: Status) -> Node<Fixed> {
        Node::leaf(Fixed(name, status))
    }

    fn run(tree: &Node<Fixed>) -> (Status, Vec<&'static str>) {
        let mut host = Script { ran: Vec::new() };
        let status = tree.tick(&mut host);
        (status, host.ran)
    }

    use Status::{Failure, Running, Success};

    #[test]
    fn a_leaf_reports_what_the_host_says() {
        for status in [Success, Failure, Running] {
            assert_eq!(run(&leaf("a", status)).0, status);
        }
    }

    #[test]
    fn a_sequence_stops_at_the_first_child_that_is_not_a_success() {
        let tree = Node::sequence(vec![leaf("a", Success), leaf("b", Failure), leaf("c", Success)]);
        assert_eq!(run(&tree), (Failure, vec!["a", "b"]));
        let tree = Node::sequence(vec![leaf("a", Success), leaf("b", Running), leaf("c", Success)]);
        assert_eq!(run(&tree), (Running, vec!["a", "b"]));
        let tree = Node::sequence(vec![leaf("a", Success), leaf("b", Success)]);
        assert_eq!(run(&tree), (Success, vec!["a", "b"]));
    }

    #[test]
    fn a_selector_stops_at_the_first_child_that_is_not_a_failure() {
        let tree = Node::selector(vec![leaf("a", Failure), leaf("b", Success), leaf("c", Success)]);
        assert_eq!(run(&tree), (Success, vec!["a", "b"]));
        let tree = Node::selector(vec![leaf("a", Failure), leaf("b", Running), leaf("c", Success)]);
        assert_eq!(run(&tree), (Running, vec!["a", "b"]));
        let tree = Node::selector(vec![leaf("a", Failure), leaf("b", Failure)]);
        assert_eq!(run(&tree), (Failure, vec!["a", "b"]));
    }

    #[test]
    fn not_swaps_success_and_failure_but_leaves_running() {
        assert_eq!(run(&Node::not(leaf("a", Success))).0, Failure);
        assert_eq!(run(&Node::not(leaf("a", Failure))).0, Success);
        assert_eq!(run(&Node::not(leaf("a", Running))).0, Running);
    }

    #[test]
    fn always_turns_failure_into_success_only() {
        assert_eq!(run(&Node::always(leaf("a", Failure))).0, Success);
        assert_eq!(run(&Node::always(leaf("a", Success))).0, Success);
        assert_eq!(run(&Node::always(leaf("a", Running))).0, Running);
    }

    #[test]
    fn trees_nest() {
        // (a fails) or ((b succeeds) and (c is running))
        let tree = Node::selector(vec![leaf("a", Failure), Node::sequence(vec![leaf("b", Success), leaf("c", Running)]), leaf("d", Success)]);
        assert_eq!(run(&tree), (Running, vec!["a", "b", "c"]));
    }

    #[test]
    fn every_tick_starts_again_from_the_root() {
        // A host whose first leaf changes its mind: the tree reacts at once, with no memory of
        // having been in the second branch.
        struct Flip(bool);
        impl Host<&'static str> for Flip {
            fn run(&mut self, leaf: &&'static str) -> Status {
                match *leaf {
                    "interrupt" if self.0 => Status::Success,
                    "interrupt" => Status::Failure,
                    "react" => Status::Running,
                    _ => Status::Running,
                }
            }
        }
        let tree = Node::selector(vec![
            Node::sequence(vec![Node::leaf("interrupt"), Node::leaf("react")]),
            Node::leaf("carry_on"),
        ]);
        let mut host = Flip(false);
        assert_eq!(tree.tick(&mut host), Status::Running);
        host.0 = true;
        assert_eq!(tree.tick(&mut host), Status::Running);
    }

    #[test]
    fn empty_composites_are_refused() {
        let empty: Node<Fixed> = Node::sequence(vec![]);
        assert_eq!(empty.validate(), Err(TreeError::EmptyComposite));
        let nested = Node::selector(vec![leaf("a", Success), Node::sequence(vec![])]);
        assert_eq!(nested.validate(), Err(TreeError::EmptyComposite));
        assert_eq!(Node::not(Node::<Fixed>::selector(vec![])).validate(), Err(TreeError::EmptyComposite));
    }

    #[test]
    fn a_tree_may_be_sixteen_levels_deep_but_not_seventeen() {
        let nest = |levels: usize| {
            let mut node = leaf("a", Success);
            for _ in 1..levels {
                node = Node::always(node);
            }
            node
        };
        assert_eq!(nest(MAX_DEPTH).validate(), Ok(()));
        assert_eq!(nest(MAX_DEPTH + 1).validate(), Err(TreeError::TooDeep));
    }

    #[test]
    fn leaves_are_counted_and_errors_read_plainly() {
        let tree = Node::selector(vec![leaf("a", Success), Node::sequence(vec![leaf("b", Success), Node::not(leaf("c", Success))])]);
        assert_eq!(tree.leaf_count(), 3);
        assert!(TreeError::EmptyComposite.to_string().contains("no children"));
        assert!(TreeError::TooDeep.to_string().contains("16"));
    }
}
