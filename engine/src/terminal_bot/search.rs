use super::no_suicide::SearchChoices;
use super::*;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Mcts,
    Uniform,
}
#[derive(Clone, Copy, Debug)]
pub enum Budget {
    Trials(u64),
    Time(Duration),
}
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub method: Method,
    pub budget: Budget,
    pub seed: u64,
    pub max_trial_steps: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            method: Method::Mcts,
            budget: Budget::Time(Duration::from_secs(20)),
            seed: 1,
            max_trial_steps: 100_000,
        }
    }
}
#[derive(Clone, Debug, Default)]
struct Edge {
    index: u128,
    visits: u64,
    wins: u64,
    draws: u64,
    losses: u64,
    prior: f64,
}
impl Edge {
    fn value(&self) -> f64 {
        (self.wins as f64 + 0.5 * self.draws as f64) / self.visits.max(1) as f64
    }
    fn record(&mut self, outcome: Outcome, p: Player) {
        self.visits += 1;
        match outcome {
            Outcome::Draw => self.draws += 1,
            Outcome::Won { winner } if winner == p => self.wins += 1,
            Outcome::Conceded { loser } if loser != p => self.wins += 1,
            _ => self.losses += 1,
        }
    }
}
#[derive(Default)]
struct Node {
    edges: Vec<Edge>,
    prior: Option<Vec<f64>>,
    visits: u64,
}
impl Node {
    fn select(
        &mut self,
        choices: &SearchChoices,
        rng: &mut Random,
        method: Method,
    ) -> (usize, bool) {
        let n = choices.len();
        if (self.edges.len() as u128) < n {
            let mut rank = rng.below(n);
            while self.edges.iter().any(|e| e.index == choices.index(rank)) {
                rank = (rank + 1) % n;
            }
            let index = choices.index(rank);
            self.edges.push(Edge {
                index,
                prior: self.prior.as_ref().map_or(0., |p| p[rank as usize]),
                ..Default::default()
            });
            return (self.edges.len() - 1, true);
        }
        let choice = match method {
            Method::Uniform => {
                let min = self.edges.iter().map(|e| e.visits).min().unwrap();
                let options = self
                    .edges
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.visits == min)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                options[rng.below(options.len() as u128) as usize]
            }
            Method::Mcts => {
                let log = (self.visits.max(1) as f64).ln();
                let mut best = f64::NEG_INFINITY;
                let mut options = Vec::new();
                for (i, e) in self.edges.iter().enumerate() {
                    // c_puct=1, as validated. Large action spaces retain UCB.
                    let v = e.value()
                        + if self.prior.is_some() {
                            e.prior * (self.visits as f64).sqrt() / (1. + e.visits as f64)
                        } else {
                            (2.0 * log / e.visits.max(1) as f64).sqrt()
                        };
                    if v > best {
                        best = v;
                        options.clear();
                        options.push(i);
                    } else if v == best {
                        options.push(i);
                    }
                }
                options[rng.below(options.len() as u128) as usize]
            }
        };
        (choice, false)
    }
}
#[derive(Clone, Debug)]
pub struct SearchReport {
    pub selected: Option<Action>,
    pub completed: u64,
    pub incomplete: u64,
    pub steps: u64,
    pub automatic_steps: u64,
    pub rollout_decisions: u64,
    pub pruned_actions: u64,
    pub nodes: usize,
    pub elapsed_seconds: f64,
    pub error: Option<String>,
    pub details: Value,
}
impl SearchReport {
    pub fn json(&self) -> Value {
        json!({"control":"auto","selected":self.selected.map(action_text),"completed":self.completed,"incomplete":self.incomplete,"steps":self.steps,"automatic_steps":self.automatic_steps,"rollout_decisions":self.rollout_decisions,"pruned_actions":self.pruned_actions,"nodes":self.nodes,"elapsed_seconds":self.elapsed_seconds,"error":self.error,"details":self.details})
    }
}
/// Resumable search. A session belongs to exactly one observer-visible root.
/// Callers may split a fixed trial budget without changing RNG or tree policy.
pub struct SearchSession {
    observation: Observation,
    config: Config,
    report: SearchReport,
    table: HashMap<Observation, usize>,
    nodes: Vec<Node>,
    sample_rng: Random,
    policy_rng: Random,
    chance_rng: Random,
    root_search_count: u128,
    pub memory_limit: usize,
    pub interruptible: bool,
}
fn empty_report() -> SearchReport {
    SearchReport {
        selected: None,
        completed: 0,
        incomplete: 0,
        steps: 0,
        automatic_steps: 0,
        rollout_decisions: 0,
        pruned_actions: 0,
        nodes: 0,
        elapsed_seconds: 0.0,
        error: None,
        details: json!({}),
    }
}
impl SearchSession {
    pub fn new(observation: &Observation, config: Config) -> std::result::Result<Self, String> {
        let actions = observation
            .auto_actions()
            .map_err(|e| format!("root actions: {e:?}"))?;
        let mut table = HashMap::new();
        table.insert(observation.clone(), 0);
        Ok(Self {
            observation: observation.clone(),
            config,
            report: empty_report(),
            table,
            nodes: vec![Node::default()],
            sample_rng: Random(config.seed ^ 0x73616d706c65),
            policy_rng: Random(config.seed ^ 0x706f6c696379),
            chance_rng: Random(config.seed ^ 0x6368616e6365),
            root_search_count: actions.len(),
            memory_limit: usize::MAX,
            interruptible: false,
        })
    }
    pub fn observation(&self) -> &Observation {
        &self.observation
    }
    pub fn elapsed(&self) -> f64 {
        self.report.elapsed_seconds
    }
    pub fn memory_bytes(&self) -> usize {
        self.table.capacity() * (std::mem::size_of::<Observation>() + 32)
            + self.nodes.capacity() * std::mem::size_of::<Node>()
            + self
                .nodes
                .iter()
                .map(|n| {
                    n.edges.capacity() * std::mem::size_of::<Edge>()
                        + n.prior.as_ref().map_or(0, |p| p.capacity() * 8)
                })
                .sum::<usize>()
    }
    pub fn run(&mut self, budget: Budget) -> SearchReport {
        if self.report.error.is_some() {
            return self.report.clone();
        }
        let start = Instant::now();
        let observation = &self.observation;
        let config = Config {
            budget,
            ..self.config
        };
        let actions = observation.auto_actions().expect("validated root");
        let mut report = self.report.clone();
        let before_completed = report.completed;
        let before_elapsed = report.elapsed_seconds;
        let mut root_search_count = self.root_search_count;
        let mut table = std::mem::take(&mut self.table);
        let mut nodes = std::mem::take(&mut self.nodes);
        let mut sample_rng = self.sample_rng.clone();
        let mut policy_rng = self.policy_rng.clone();
        let mut chance_rng = self.chance_rng.clone();
        // Conservative per-node budget, including hash-table slack and edges.
        let max_nodes =
            (self.memory_limit / (4 * std::mem::size_of::<Observation>() + 16384)).max(1);
        loop {
            let more = match config.budget {
                Budget::Trials(n) => report.completed - before_completed < n,
                Budget::Time(d) => start.elapsed() < d,
            };
            if !more {
                break;
            }
            let attempt = (|| -> Result<Outcome> {
                let mut position = observation.sample(&mut sample_rng)?;
                let mut path = Vec::<(usize, usize, Player)>::new();
                let mut rollout = false;
                let mut trial_steps = 0;
                loop {
                    if let Some(outcome) = position.engine.finished() {
                        for (node, edge, actor) in path {
                            nodes[node].visits += 1;
                            nodes[node].edges[edge].record(outcome, actor);
                        }
                        return Ok(outcome);
                    }
                    if trial_steps >= config.max_trial_steps {
                        return Err(Error::Illegal("trial watchdog: unfinished, no reward"));
                    }
                    if self.interruptible && trial_steps % 64 == 0 {
                        if let Budget::Time(d) = config.budget {
                            if start.elapsed() >= d {
                                return Err(Error::Illegal("search slice interrupted"));
                            }
                        }
                    }
                    trial_steps += 1;
                    report.steps += 1;
                    if chance_owner(&position.engine).is_some() {
                        resolve_chance(&mut position, &mut chance_rng)?;
                        continue;
                    }
                    let (actor, space) = match auto_step(&position.engine)? {
                        AutoStep::Automatic { actor, action, .. } => {
                            report.automatic_steps += 1;
                            position.apply(actor, action)?;
                            continue;
                        }
                        AutoStep::Decision { actor, actions } => (actor, actions),
                        _ => return Err(Error::Illegal("unexpected auto boundary")),
                    };
                    let choices = SearchChoices::new(&position.engine, actor, &space)?;
                    report.pruned_actions += (space.len() - choices.len()) as u64;
                    let index = if rollout {
                        report.rollout_decisions += 1;
                        rollout_policy::choose(
                            &position.engine,
                            actor,
                            &space,
                            &choices,
                            &mut policy_rng,
                        )
                    } else {
                        let id = if path.is_empty() {
                            // Every sampled trial starts at this known root.
                            0
                        } else {
                            let key = position.observe(actor);
                            if let Some(&id) = table.get(&key) {
                                id
                            } else {
                                if nodes.len() >= max_nodes {
                                    rollout = true;
                                    let index = rollout_policy::choose(
                                        &position.engine,
                                        actor,
                                        &space,
                                        &choices,
                                        &mut policy_rng,
                                    );
                                    position.apply(actor, space.get(index))?;
                                    continue;
                                }
                                let id = nodes.len();
                                nodes.push(Node::default());
                                table.insert(key, id);
                                id
                            }
                        };
                        if id == 0 {
                            root_search_count = choices.len();
                        }
                        // Cache once per observation node, immediately before its
                        // first edge. Uniform root allocation needs no policy prior.
                        if config.method == Method::Mcts && nodes[id].edges.is_empty() {
                            nodes[id].prior =
                                rollout_policy::prior(&position.engine, actor, &space, &choices);
                        }
                        let (edge, new) = nodes[id].select(
                            &choices,
                            &mut policy_rng,
                            if id == 0 { config.method } else { Method::Mcts },
                        );
                        path.push((id, edge, actor));
                        if new || config.method == Method::Uniform {
                            rollout = true;
                        }
                        nodes[id].edges[edge].index
                    };
                    position.apply(actor, space.get(index))?;
                }
            })();
            match attempt {
                Ok(_) => report.completed += 1,
                Err(e) => {
                    report.incomplete += 1;
                    if matches!(e, Error::Illegal("search slice interrupted")) {
                        break;
                    }
                    report.error = Some(format!("trial {}: {e:?}", report.completed));
                    break;
                }
            }
        }
        report.nodes = nodes.len();
        report.elapsed_seconds = before_elapsed + start.elapsed().as_secs_f64();
        if report.error.is_none() && report.completed > 0 {
            let root = &nodes[0];
            let best = root
                .edges
                .iter()
                .max_by(|a, b| {
                    // Equal-allocation comparator chooses mean; MCTS chooses most visited.
                    let primary = if config.method == Method::Mcts {
                        a.visits.cmp(&b.visits)
                    } else {
                        a.value().total_cmp(&b.value())
                    };
                    primary
                        .then_with(|| a.value().total_cmp(&b.value()))
                        .then_with(|| b.index.cmp(&a.index))
                })
                .unwrap();
            report.selected = Some(actions.get(best.index));
        }
        if report.error.is_none() && report.completed == 0 && !self.interruptible {
            report.error = Some("budget completed no trials".into());
        }
        let mut edges = nodes[0].edges.clone();
        edges.sort_by_key(|e| e.index);
        report.details = json!({"method":format!("{:?}",config.method),"budget":format!("{:?}",config.budget),"seed":config.seed,"watchdog_steps":config.max_trial_steps,"rollout_policy":"expert-g1-8103: 80% greedy, 20% uniform; >64 choices uniform","legal_count":actions.len().to_string(),"search_count":root_search_count.to_string(),"search_policy":"exclude cost suicide, lethal Eidolon casts and lethal self burn; no resignation","visited_candidates":edges.len(),"reward":"terminal win=1 draw=0.5 loss=0; rollout-policy estimate, not an optimal-play probability","candidates":edges.iter().map(|e|json!({"index":e.index.to_string(),"action":action_text(actions.get(e.index)),"visits":e.visits,"wins":e.wins,"draws":e.draws,"losses":e.losses,"mean":e.value()})).collect::<Vec<_>>()});
        report.details["tree_policy"] = json!(if config.method == Method::Mcts {
            "expert-g1-8103 PUCT; c=1; softmax temperature=1; 90% prior + 10% uniform; >64 choices UCB"
        } else {
            "uniform root allocation"
        });
        if config.method == Method::Mcts {
            report.details["prior_nodes"] =
                json!(nodes.iter().filter(|n| n.prior.is_some()).count());
            report.details["c_puct"] = json!(1.0);
            report.details["root_priors"] = json!(
                edges
                    .iter()
                    .map(|e| json!({"index":e.index.to_string(),"p":e.prior}))
                    .collect::<Vec<_>>()
            );
        }
        self.table = table;
        self.nodes = nodes;
        self.sample_rng = sample_rng;
        self.policy_rng = policy_rng;
        self.chance_rng = chance_rng;
        self.root_search_count = root_search_count;
        self.report = report.clone();
        report
    }
}
pub fn search(observation: &Observation, config: Config) -> SearchReport {
    match SearchSession::new(observation, config) {
        Ok(mut session) => session.run(config.budget),
        Err(error) => {
            let mut report = empty_report();
            report.error = Some(error);
            report
        }
    }
}

/// Forecast from a redacted bot observation only. Never accepts the live world.
pub fn ponder_candidate(observation: &Observation, seed: u64) -> Option<Observation> {
    let mut rng = Random(seed);
    let mut position = observation.sample(&mut rng).ok()?;
    for _ in 0..4096 {
        match auto_step(&position.engine).ok()? {
            AutoStep::Finished(_) => return None,
            AutoStep::Chance => {
                resolve_chance(&mut position, &mut rng).ok()?;
            }
            AutoStep::Automatic { actor, action, .. } => {
                position.apply(actor, action).ok()?;
            }
            AutoStep::Decision { actor, actions } => {
                if actor == observation.viewer {
                    return Some(position.observe(actor));
                }
                let choices = SearchChoices::new(&position.engine, actor, &actions).ok()?;
                let i =
                    rollout_policy::choose(&position.engine, actor, &actions, &choices, &mut rng);
                position.apply(actor, actions.get(i)).ok()?;
            }
        }
    }
    None
}
