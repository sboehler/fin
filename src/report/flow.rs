//! Flow report: money moving between accounts over a period.
//!
//! Every booking carries its counter-account (`Entry::other`), so the journal
//! is already a flow graph: an entry with a positive amount is an edge
//! `other -> account`. Bookings are created in credit/debit pairs, so keeping
//! only the positive side both fixes the direction and avoids counting each
//! leg twice.
//!
//! The raw graph is then cleaned up for rendering:
//!
//! - accounts are projected through the [`AccountMapper`] (`-m`/`-r`), which
//!   is what keeps the chart down to a readable number of nodes,
//! - edges that became self-edges under that projection are dropped,
//! - flows between the same two accounts are netted against each other,
//! - edges below `min` are pruned,
//! - any remaining cycles are broken, since sankey layout needs a DAG.

use std::collections::HashMap;

use chrono::NaiveDate;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde_json::json;

use crate::model::{
    entities::{AccountID, AccountType, CommodityID},
    journal::Journal,
};

use super::mapping::AccountMapper;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub value: Decimal,
}

/// The role an account plays in a flow chart. Renderers colour by this rather
/// than by account type: a sankey distinguishes where money comes from, where
/// it sits and where it goes, and three categories is also as many as a
/// categorical palette can keep apart for colour-blind readers when every node
/// is compared against every other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Source,
    Hub,
    Sink,
}

impl Category {
    fn as_str(&self) -> &'static str {
        match self {
            Category::Source => "source",
            Category::Hub => "hub",
            Category::Sink => "sink",
        }
    }
}

impl From<AccountType> for Category {
    fn from(account_type: AccountType) -> Self {
        match account_type {
            AccountType::Income => Category::Source,
            AccountType::Expenses => Category::Sink,
            AccountType::Assets | AccountType::Liabilities | AccountType::Equity => Category::Hub,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub category: Category,
}

#[derive(Debug, Default)]
pub struct FlowReport {
    /// Accounts, sorted by name. [`Edge`] endpoints index into this.
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Non-fatal problems to report on stderr, e.g. cycles that were broken.
    pub warnings: Vec<String>,
}

impl FlowReport {
    /// Describes this report as an ECharts `option` object, the intermediate
    /// every chart type renders through.
    pub fn to_sankey_option(&self) -> serde_json::Value {
        json!({
            "tooltip": {
                "trigger": "item",
                "triggerOn": "mousemove",
            },
            "series": [{
                "type": "sankey",
                "nodeAlign": "justify",
                "emphasis": { "focus": "adjacency" },
                "lineStyle": { "color": "gradient", "curveness": 0.5 },
                // `category` is not an ECharts field: the renderer maps it to a
                // colour, so the palette lives with the theme rather than here.
                "data": self.nodes.iter()
                    .map(|node| json!({
                        "name": node.name,
                        "category": node.category.as_str(),
                    }))
                    .collect::<Vec<_>>(),
                "links": self.edges.iter()
                    .map(|edge| json!({
                        "source": self.nodes[edge.from].name,
                        "target": self.nodes[edge.to].name,
                        "value": to_f64(edge.value),
                    }))
                    .collect::<Vec<_>>(),
            }],
        })
    }
}

/// Rounds to cents so chart output does not depend on valuation precision.
fn to_f64(value: Decimal) -> f64 {
    value
        .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero)
        .to_f64()
        .unwrap_or_default()
}

pub struct FlowBuilder {
    pub from: Option<NaiveDate>,
    pub to: NaiveDate,
    pub mapper: AccountMapper,
    /// Whether the journal was valuated; if so, edges use values rather than
    /// quantities.
    pub valuated: bool,
    pub min: Option<Decimal>,
    /// Whether to give each hub account's retained amount an explicit edge, so
    /// that the chart conserves.
    pub balance: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error(
        "journal uses more than one commodity ({0}, {1}, ...): pass --valuation to convert flows to a common commodity"
    )]
    MixedCommodities(String, String),
}

impl FlowBuilder {
    pub fn build(&self, journal: &Journal) -> Result<FlowReport, FlowError> {
        let from = self.from.or(journal.min_transaction_date());
        let registry = journal.registry();

        // Accumulate directed flows between mapped accounts.
        let mut flows = HashMap::<(AccountID, AccountID), Decimal>::new();
        let mut commodity: Option<CommodityID> = None;
        for entry in journal.query() {
            if from.is_some_and(|from| entry.date < from) || entry.date > self.to {
                continue;
            }
            let amount = match self.valuated {
                true => entry.value.unwrap_or_default(),
                false => entry.quantity,
            };
            // Keeps one side of each credit/debit pair, oriented other -> account.
            if amount <= Decimal::ZERO {
                continue;
            }
            if !self.valuated {
                match commodity {
                    Some(c) if c != entry.commodity => {
                        return Err(FlowError::MixedCommodities(
                            registry.commodity_name(c),
                            registry.commodity_name(entry.commodity),
                        ));
                    }
                    Some(_) => {}
                    None => commodity = Some(entry.commodity),
                }
            }
            let (Some(source), Some(target)) = (
                self.mapper.map(registry, entry.other),
                self.mapper.map(registry, entry.account),
            ) else {
                continue;
            };
            if source == target {
                continue;
            }
            *flows.entry((source, target)).or_default() += amount;
        }

        let flows = Self::net(flows);
        let mut nodes = flows
            .keys()
            .flat_map(|(source, target)| [*source, *target])
            .collect::<Vec<_>>();
        nodes.sort_by_key(|account| registry.account_name(*account));
        nodes.dedup();
        let index = nodes
            .iter()
            .enumerate()
            .map(|(i, account)| (*account, i))
            .collect::<HashMap<_, _>>();

        let mut edges = flows
            .into_iter()
            .filter(|(_, value)| self.min.is_none_or(|min| *value >= min))
            .map(|((source, target), value)| Edge {
                from: index[&source],
                to: index[&target],
                value,
            })
            .collect::<Vec<_>>();
        edges.sort_by_key(|e| (e.from, e.to));

        let nodes = nodes
            .into_iter()
            .map(|account| Node {
                name: registry.account_name(account),
                category: account.account_type.into(),
            })
            .collect::<Vec<_>>();
        let warnings = Self::break_cycles(nodes.len(), &nodes, &mut edges);
        let mut nodes = nodes;
        if self.balance {
            // After pruning and cycle breaking, so it reflects the flows that
            // are actually drawn.
            Self::add_balance_edges(&mut nodes, &mut edges);
        }
        let nodes = Self::compact(nodes, &mut edges);

        Ok(FlowReport {
            nodes,
            edges,
            warnings,
        })
    }

    /// Money that stays put is not a flow, so it has nowhere to go in the
    /// chart: an account takes in more than it puts out and the difference
    /// simply stops at the node, unlabelled. That is exactly the amount saved
    /// (or, the other way round, drawn down), so give it an edge of its own
    /// and let it be read like any other flow.
    ///
    /// Only hub accounts get one. Expenses are where money is meant to end up
    /// and income is where it comes from, so neither is unbalanced in a way
    /// worth drawing.
    ///
    /// The two synthetic nodes cannot create a cycle: one only ever has
    /// outgoing edges and the other only incoming.
    fn add_balance_edges(nodes: &mut Vec<Node>, edges: &mut Vec<Edge>) {
        let mut net = vec![Decimal::ZERO; nodes.len()];
        for edge in edges.iter() {
            net[edge.from] -= edge.value;
            net[edge.to] += edge.value;
        }
        let unbalanced = |sign: std::cmp::Ordering| -> Vec<(usize, Decimal)> {
            net.iter()
                .enumerate()
                .filter(|(i, value)| {
                    nodes[*i].category == Category::Hub && value.cmp(&&Decimal::ZERO) == sign
                })
                .map(|(i, value)| (i, value.abs()))
                .collect()
        };
        // Both sides are collected before either synthetic node is pushed, so
        // the scan never sees a node it just added.
        let retained = unbalanced(std::cmp::Ordering::Greater);
        let drawn_down = unbalanced(std::cmp::Ordering::Less);

        if !retained.is_empty() {
            let sink = Self::push_node(nodes, "Net change");
            edges.extend(retained.into_iter().map(|(from, value)| Edge {
                from,
                to: sink,
                value,
            }));
        }
        if !drawn_down.is_empty() {
            let source = Self::push_node(nodes, "Opening balance");
            edges.extend(drawn_down.into_iter().map(|(to, value)| Edge {
                from: source,
                to,
                value,
            }));
        }
        edges.sort_by_key(|e| (e.from, e.to));
    }

    /// Synthetic nodes are coloured as hubs: what an account held is closer to
    /// an asset than to income or spending. Account names cannot contain
    /// spaces, so these cannot collide with a real one.
    fn push_node(nodes: &mut Vec<Node>, name: &str) -> usize {
        nodes.push(Node {
            name: name.to_string(),
            category: Category::Hub,
        });
        nodes.len() - 1
    }

    /// Collapses `a -> b` and `b -> a` into a single edge in the direction of
    /// the larger flow. Money moving back and forth between two accounts is
    /// both the common case for cycles and misleading to show as two ribbons.
    fn net(
        flows: HashMap<(AccountID, AccountID), Decimal>,
    ) -> HashMap<(AccountID, AccountID), Decimal> {
        let mut netted = HashMap::<(AccountID, AccountID), Decimal>::new();
        for ((source, target), value) in flows {
            if let Some(back) = netted.remove(&(target, source)) {
                let net = back - value;
                match net.cmp(&Decimal::ZERO) {
                    std::cmp::Ordering::Greater => netted.insert((target, source), net),
                    std::cmp::Ordering::Less => netted.insert((source, target), -net),
                    // Flows cancel exactly; drop the edge.
                    std::cmp::Ordering::Equal => None,
                };
            } else {
                *netted.entry((source, target)).or_default() += value;
            }
        }
        netted
    }

    /// Drops nodes that no longer have any edge and reindexes the remaining
    /// ones. Both `min` pruning and cycle breaking can orphan a node, and a
    /// node without edges renders as a stray zero-height bar.
    fn compact(nodes: Vec<Node>, edges: &mut [Edge]) -> Vec<Node> {
        let mut used = vec![false; nodes.len()];
        for edge in edges.iter() {
            used[edge.from] = true;
            used[edge.to] = true;
        }
        let mut indices = vec![0; nodes.len()];
        let mut kept = Vec::new();
        for (i, node) in nodes.into_iter().enumerate() {
            if used[i] {
                indices[i] = kept.len();
                kept.push(node);
            }
        }
        for edge in edges.iter_mut() {
            edge.from = indices[edge.from];
            edge.to = indices[edge.to];
        }
        kept
    }

    /// Drops the smallest edge of each remaining cycle until the graph is a
    /// DAG, which is what the sankey layout needs.
    fn break_cycles(len: usize, nodes: &[Node], edges: &mut Vec<Edge>) -> Vec<String> {
        let mut warnings = Vec::new();
        while let Some(cycle) = find_cycle(len, edges) {
            let weakest = *cycle
                .iter()
                .min_by_key(|i| edges[**i].value)
                .expect("a cycle has at least one edge");
            let path = cycle
                .iter()
                .map(|i| nodes[edges[*i].from].name.as_str())
                .collect::<Vec<_>>()
                .join(" -> ");
            let dropped = &edges[weakest];
            warnings.push(format!(
                "dropped {} -> {} ({}) to break cycle {path}",
                nodes[dropped.from].name, nodes[dropped.to].name, dropped.value
            ));
            edges.remove(weakest);
        }
        warnings
    }
}

/// Returns the edge indices forming some cycle, or `None` if the graph is
/// acyclic.
fn find_cycle(len: usize, edges: &[Edge]) -> Option<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); len];
    for (i, edge) in edges.iter().enumerate() {
        adjacency[edge.from].push(i);
    }
    let mut colors = vec![Color::White; len];
    let mut path = Vec::new();
    (0..len).find_map(|node| visit(node, &adjacency, edges, &mut colors, &mut path))
}

#[derive(Clone, Copy, PartialEq)]
enum Color {
    White,
    Gray,
    Black,
}

fn visit(
    node: usize,
    adjacency: &[Vec<usize>],
    edges: &[Edge],
    colors: &mut Vec<Color>,
    path: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    if colors[node] != Color::White {
        return None;
    }
    colors[node] = Color::Gray;
    for &edge in &adjacency[node] {
        let next = edges[edge].to;
        path.push(edge);
        if colors[next] == Color::Gray {
            let start = path
                .iter()
                .position(|e| edges[*e].from == next)
                .expect("the gray node is on the current path");
            return Some(path[start..].to_vec());
        }
        if let Some(cycle) = visit(next, adjacency, edges, colors, path) {
            return Some(cycle);
        }
        path.pop();
    }
    colors[node] = Color::Black;
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn account(id: usize) -> AccountID {
        AccountID {
            account_type: AccountType::Assets,
            id,
        }
    }

    fn dec(v: i64) -> Decimal {
        Decimal::new(v, 0)
    }

    fn edge(from: usize, to: usize, value: i64) -> Edge {
        Edge {
            from,
            to,
            value: dec(value),
        }
    }

    fn nodes(n: usize) -> Vec<Node> {
        (0..n)
            .map(|i| Node {
                name: format!("A{i}"),
                category: Category::Hub,
            })
            .collect()
    }

    #[test]
    fn net_collapses_opposing_flows() {
        let flows = HashMap::from([
            ((account(0), account(1)), dec(2000)),
            ((account(1), account(0)), dec(500)),
        ]);
        assert_eq!(
            FlowBuilder::net(flows),
            HashMap::from([((account(0), account(1)), dec(1500))])
        );
    }

    #[test]
    fn net_flips_direction_when_the_back_flow_is_larger() {
        let flows = HashMap::from([
            ((account(0), account(1)), dec(500)),
            ((account(1), account(0)), dec(2000)),
        ]);
        assert_eq!(
            FlowBuilder::net(flows),
            HashMap::from([((account(1), account(0)), dec(1500))])
        );
    }

    #[test]
    fn net_drops_flows_that_cancel_exactly() {
        let flows = HashMap::from([
            ((account(0), account(1)), dec(500)),
            ((account(1), account(0)), dec(500)),
        ]);
        assert!(FlowBuilder::net(flows).is_empty());
    }

    #[test]
    fn find_cycle_returns_none_for_a_dag() {
        // 0 -> 1 -> 2, plus a shortcut 0 -> 2: diamond, not a cycle.
        let edges = vec![edge(0, 1, 10), edge(1, 2, 10), edge(0, 2, 10)];
        assert_eq!(find_cycle(3, &edges), None);
    }

    #[test]
    fn break_cycles_drops_the_weakest_edge() {
        // 0 -> 1 -> 2 -> 0, with 1 -> 2 the weakest.
        let mut edges = vec![edge(0, 1, 30), edge(1, 2, 10), edge(2, 0, 20)];
        let warnings = FlowBuilder::break_cycles(3, &nodes(3), &mut edges);
        assert_eq!(edges, vec![edge(0, 1, 30), edge(2, 0, 20)]);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].starts_with("dropped A1 -> A2 (10) to break cycle"),
            "unexpected warning: {}",
            warnings[0]
        );
    }

    #[test]
    fn break_cycles_leaves_a_dag_untouched() {
        let mut edges = vec![edge(0, 1, 30), edge(1, 2, 10), edge(0, 2, 20)];
        let expected = edges.clone();
        assert!(FlowBuilder::break_cycles(3, &nodes(3), &mut edges).is_empty());
        assert_eq!(edges, expected);
    }

    #[test]
    fn break_cycles_handles_two_independent_cycles() {
        // 0 -> 1 -> 0 and 2 -> 3 -> 2.
        let mut edges = vec![
            edge(0, 1, 30),
            edge(1, 0, 10),
            edge(2, 3, 5),
            edge(3, 2, 40),
        ];
        let warnings = FlowBuilder::break_cycles(4, &nodes(4), &mut edges);
        assert_eq!(edges, vec![edge(0, 1, 30), edge(3, 2, 40)]);
        assert_eq!(warnings.len(), 2);
    }

    fn node(name: &str, category: Category) -> Node {
        Node {
            name: name.to_string(),
            category,
        }
    }

    #[test]
    fn balance_edges_route_what_a_hub_retained_to_a_sink() {
        // Income 100 -> hub, hub -> expenses 30: the other 70 stayed put.
        let mut nodes = vec![
            node("Income:Salary", Category::Source),
            node("Assets:Bank", Category::Hub),
            node("Expenses:Food", Category::Sink),
        ];
        let mut edges = vec![edge(0, 1, 100), edge(1, 2, 30)];
        FlowBuilder::add_balance_edges(&mut nodes, &mut edges);
        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[3], node("Net change", Category::Hub));
        assert_eq!(edges, vec![edge(0, 1, 100), edge(1, 2, 30), edge(1, 3, 70)]);
    }

    #[test]
    fn balance_edges_route_a_drawn_down_hub_from_a_source() {
        // The hub pays out 30 without taking anything in this period.
        let mut nodes = vec![
            node("Assets:Bank", Category::Hub),
            node("Expenses:Food", Category::Sink),
        ];
        let mut edges = vec![edge(0, 1, 30)];
        FlowBuilder::add_balance_edges(&mut nodes, &mut edges);
        assert_eq!(nodes[2], node("Opening balance", Category::Hub));
        assert_eq!(edges, vec![edge(0, 1, 30), edge(2, 0, 30)]);
    }

    #[test]
    fn balance_edges_leave_income_and_expenses_alone() {
        // Income is where money starts and expenses where it ends: neither is
        // unbalanced in a way worth drawing, so nothing is added.
        let mut nodes = vec![
            node("Income:Salary", Category::Source),
            node("Expenses:Food", Category::Sink),
        ];
        let mut edges = vec![edge(0, 1, 100)];
        FlowBuilder::add_balance_edges(&mut nodes, &mut edges);
        assert_eq!(nodes.len(), 2);
        assert_eq!(edges, vec![edge(0, 1, 100)]);
    }

    #[test]
    fn balance_edges_skip_a_hub_that_already_conserves() {
        let mut nodes = vec![
            node("Income:Salary", Category::Source),
            node("Assets:Bank", Category::Hub),
            node("Expenses:Food", Category::Sink),
        ];
        let mut edges = vec![edge(0, 1, 100), edge(1, 2, 100)];
        FlowBuilder::add_balance_edges(&mut nodes, &mut edges);
        assert_eq!(nodes.len(), 3);
        assert_eq!(edges, vec![edge(0, 1, 100), edge(1, 2, 100)]);
    }

    #[test]
    fn compact_drops_orphaned_nodes_and_reindexes() {
        // Node 1 has no edges left after pruning; 0 and 2 survive.
        let mut edges = vec![edge(0, 2, 10)];
        let kept = FlowBuilder::compact(nodes(3), &mut edges);
        assert_eq!(
            kept.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            vec!["A0", "A2"]
        );
        assert_eq!(edges, vec![edge(0, 1, 10)]);
    }

    #[test]
    fn break_cycles_handles_a_self_loop() {
        let mut edges = vec![edge(0, 0, 10), edge(0, 1, 20)];
        FlowBuilder::break_cycles(2, &nodes(2), &mut edges);
        assert_eq!(edges, vec![edge(0, 1, 20)]);
    }
}
