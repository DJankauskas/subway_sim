use std::cell::Cell;
use std::cmp::min;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use petgraph::Graph;
use rand::rngs::StdRng;
use rand::prelude::SliceRandom;
use rand::SeedableRng;
use z3::ast::Ast;

use crate::shortest_path::{dijkstra, Terminated};
use crate::{Edge, EdgeType};

pub type SubwayMap = Graph<String, Edge>;
pub type StationId = NodeIndex<u32>;
pub type TrackId = EdgeIndex;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TrainId {
    pub route_idx: u32,
    pub count: u32,
}

impl TrainId {
    fn to_z3_departure(self, ctx: &z3::Context) -> z3::ast::Int {
        z3::ast::Int::new_const(ctx, format!("{}_{}", self.route_idx, self.count))
    }
}

#[derive(Debug, Clone)]
pub struct Train {
    pub id: TrainId,
    /// The current node or track the train is on
    pub curr_section: TrackStationId,
    /// The position of the train at its current section
    pub pos: f64,
    /// Total distance travelled by the train prior to the current section
    pub distance_travelled: f64,
    /// The current route the train is on
    pub route: RouteId,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: TrackId,
    pub length: u16,
    pub trains: VecDeque<TrainId>,
}

#[derive(Debug, Clone)]
pub struct Station {
    pub id: StationId,
    pub train: Option<TrainId>,
    pub arrival_times: HashMap<RouteId, Vec<f64>>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TrackStationId {
    Track(TrackId),
    Station(StationId),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RouteId(pub u32);

#[derive(Debug, Clone)]
pub struct Route {
    pub name: String,
    pub start_station: StationId,
    pub station_to: HashMap<StationId, TrackId>,
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub struct Simulator {
    subway_map: SubwayMap,
    routes: HashMap<RouteId, Route>,
    trains: HashMap<TrainId, Train>,
    curr_train_counts: Vec<u32>,
    stations: HashMap<StationId, Station>,
    tracks: HashMap<TrackId, Track>,
    traversal_order: Vec<TrackStationId>,
}

pub struct TrainPositions {
    pub time: u32,
    pub trains: Vec<TrainPosition>,
}

pub struct TrainPosition {
    pub id: TrainId,
    pub curr_section: TrackStationId,
    pub pos: f64,
    pub distance_travelled: f64,
}

pub struct StationStatistic {
    pub arrival_times: HashMap<RouteId, Vec<f64>>,
}

pub struct SimulationResults {
    pub train_positions: Vec<TrainPositions>,
    pub train_to_route: HashMap<TrainId, RouteId>,
    pub station_statistics: HashMap<StationId, StationStatistic>,
}

const STATION_DWELL_TIME: f64 = 0.5;
const MIN_TRAIN_DISTANCE: f64 = 2.0;
const TIME_STEP: f64 = 1.0;

fn f64_min(a: f64, b: f64) -> f64 {
    a.min(b)
}

fn f64_max(a: f64, b: f64) -> f64 {
    a.max(b)
}

impl Simulator {
    pub fn new(subway_map: SubwayMap, routes: Vec<Route>) -> Self {
        let mut stations = HashMap::with_capacity(subway_map.node_count());
        let mut tracks = HashMap::with_capacity(subway_map.edge_count());

        for node in subway_map.node_indices() {
            stations.insert(
                node,
                Station {
                    id: node,
                    train: None,
                    arrival_times: HashMap::new(),
                },
            );
        }

        for edge in subway_map.edge_references() {
            tracks.insert(
                edge.id(),
                Track {
                    id: edge.id(),
                    length: edge.weight().weight,
                    trains: VecDeque::new(),
                },
            );
        }

        let terminal_nodes = terminal_nodes(&subway_map);
        let mut queue: VecDeque<TrackStationId> = terminal_nodes
            .into_iter()
            .map(TrackStationId::Station)
            .collect();
        let mut traversal_order: Vec<TrackStationId> = Vec::new();
        let mut visited = HashSet::new();

        let mut all_route_edges = HashSet::new();

        for route in &routes {
            for edge in route.station_to.values() {
                all_route_edges.insert(*edge);
            }
        }

        'bfs: while let Some(track_station) = queue.pop_front() {
            if visited.contains(&track_station) {
                continue;
            };

            // If we get to a station where some of the edges it feeds into haven't been processed yet,
            // skip processing now, with the assumption that we'll be returning later. Note that this
            // requires all meaningful tracks to be assigned to a route.
            if let TrackStationId::Station(station) = track_station {
                for edge in subway_map
                    .edges_directed(station, Direction::Outgoing)
                    .filter(|e| all_route_edges.contains(&e.id()))
                {
                    if !visited.contains(&TrackStationId::Track(edge.id())) {
                        continue 'bfs;
                    }
                }
            }

            visited.insert(track_station);
            traversal_order.push(track_station);
            match track_station {
                TrackStationId::Track(track) => {
                    let (source, _) = subway_map.edge_endpoints(track).unwrap();
                    queue.push_back(TrackStationId::Station(source));
                }
                TrackStationId::Station(station) => subway_map
                    .edges_directed(station, Direction::Incoming)
                    .filter(|edge| all_route_edges.contains(&edge.id()))
                    .for_each(|track| queue.push_back(TrackStationId::Track(track.id()))),
            }
        }

        let routes: HashMap<_, _> = routes
            .into_iter()
            .enumerate()
            .map(|(i, route)| (RouteId(i as u32), route))
            .collect();

        Self {
            subway_map,
            curr_train_counts: vec![0; routes.len()],
            routes,
            trains: HashMap::new(),
            stations,
            tracks,
            traversal_order,
        }
    }

    fn reset(&mut self) {
        self.trains.clear();
        self.curr_train_counts = vec![0; self.routes.len()];
        for station in self.stations.values_mut() {
            station.arrival_times = HashMap::new();
            station.train = None;
        }
        for track in self.tracks.values_mut() {
            track.trains.clear();
        }
    }

    fn station_to_track(&mut self, station: StationId, mut time_left: f64) {
        if let Some(train) = &self.stations[&station].train {
            let train = *train;
            let train_mut = self.trains.get_mut(&train).unwrap();
            let distance_travelled = f64_max(f64_min(STATION_DWELL_TIME - train_mut.pos, time_left), 0.0);
            train_mut.pos += distance_travelled;
            time_left -= distance_travelled;

            if train_mut.pos < STATION_DWELL_TIME {
                return;
            }

            let route_id = train_mut.route;
            let next_track_id = self.routes[&route_id].station_to.get(&station);
            let next_track_id = match next_track_id {
                Some(next_track_id) => next_track_id,
                None => {
                    self.trains.remove(&train);
                    self.stations.get_mut(&station).unwrap().train = None;
                    return;
                }
            };
            let next_track: &mut Track = self.tracks.get_mut(next_track_id).unwrap();
            let last_train = next_track.trains.back();
            if let Some(last_train) = last_train {
                let last_train_pos = self.trains[last_train].pos;
                // only move the train off the station if there's space on the next track
                if last_train_pos >= MIN_TRAIN_DISTANCE {
                    self.stations.get_mut(&station).unwrap().train = None;
                    next_track.trains.push_back(train);
                    let pos_move = f64_min(time_left, last_train_pos - MIN_TRAIN_DISTANCE).max(0.0);
                    let train_mut = self.trains.get_mut(&train).unwrap();
                    train_mut.pos = pos_move;
                    train_mut.distance_travelled += STATION_DWELL_TIME;
                    train_mut.curr_section = TrackStationId::Track(*next_track_id);
                }
            } else {
                self.stations.get_mut(&station).unwrap().train = None;
                train_mut.curr_section = TrackStationId::Track(*next_track_id);
                self.trains.get_mut(&train).unwrap().pos = time_left;
                next_track.trains.push_back(train);
            }
        }
    }

    pub fn run(mut self, iterations: i32, frequency: u64) -> SimulationResults {
        let mut train_to_route = HashMap::new();
        let traversal_order = self.traversal_order.clone();
        println!("{:?}", traversal_order);
        for route in self.routes.values() {
            println!("{:?}", route.start_station);
        }

        let mut train_positions = Vec::new();

        let mut t = -120;

        while t < iterations {
            for track_station in &traversal_order {
                match *track_station {
                    TrackStationId::Station(station) => {
                        self.station_to_track(station, TIME_STEP);
                    }
                    TrackStationId::Track(track) => {
                        let mut i = 0;
                        let mut last_train_pos = f64::INFINITY;
                        let next_station_id = self.subway_map.edge_endpoints(track).unwrap().1;
                        while i < self.tracks.get_mut(&track).unwrap().trains.len() {
                            let track_mut = self.tracks.get_mut(&track).unwrap();
                            if self.stations[&next_station_id].train.is_some() {
                                last_train_pos = f64_min(
                                    f64_max(track_mut.length as f64 - MIN_TRAIN_DISTANCE, 0.0),
                                    last_train_pos,
                                );
                            }
                            let curr_train_id = track_mut.trains[i];
                            let curr_train_mut = self.trains.get_mut(&curr_train_id).unwrap();
                            let mut time_left = TIME_STEP;
                            let travel_distance = f64_min(
                                f64_min(
                                    time_left,
                                    f64_max(track_mut.length as f64 - curr_train_mut.pos, 0.0),
                                ),
                                f64_max(
                                    if curr_train_mut.pos + MIN_TRAIN_DISTANCE >= last_train_pos {
                                        last_train_pos - MIN_TRAIN_DISTANCE - curr_train_mut.pos
                                    } else {
                                        last_train_pos - curr_train_mut.pos
                                    },
                                    0.0,
                                ),
                            );
                            curr_train_mut.pos += travel_distance;
                            time_left -= travel_distance;
                            // we're done with the current track, and need to move into the station
                            if curr_train_mut.pos >= track_mut.length as f64
                                && self.stations[&next_station_id].train.is_none()
                            {
                                debug_assert_eq!(i, 0);
                                track_mut.trains.pop_front();
                                debug_assert!(
                                    self.stations
                                        .get_mut(&next_station_id)
                                        .unwrap()
                                        .train
                                        .is_none(),
                                    "travel distance is {travel_distance}"
                                );
                                let next_station_mut =
                                    self.stations.get_mut(&next_station_id).unwrap();
                                next_station_mut.train = Some(curr_train_id);
                                if t >= 0 {
                                    next_station_mut
                                        .arrival_times
                                        .entry(curr_train_mut.route)
                                        .or_default()
                                        .push(t as f64 + travel_distance);
                                }

                                curr_train_mut.distance_travelled +=
                                    self.tracks[&track].length as f64;

                                curr_train_mut.curr_section =
                                    TrackStationId::Station(next_station_id);
                                curr_train_mut.pos = 0.0;

                                self.station_to_track(next_station_id, time_left);
                                // TODO: handle the fact that some station time may be wasted when the train could keep moving
                                // potentially some of this spaghetti code needs to get factored out into
                                // a separate function, tbd
                            } else {
                                last_train_pos = curr_train_mut.pos;
                                i += 1;
                            }
                        }
                    }
                }
            }

            for (id, route) in &self.routes {
                if (t - route.offset as i32) % (frequency as i32) != 0 {
                    continue;
                }
                let start_station_mut = self.stations.get_mut(&route.start_station).unwrap();
                // TODO: do I need to handle the case where this is not true?
                let curr_train_id = TrainId {
                    route_idx: id.0,
                    count: self.curr_train_counts[id.0 as usize],
                };
                if start_station_mut.train.is_none() {
                    let train = Train {
                        id: curr_train_id,
                        curr_section: TrackStationId::Station(start_station_mut.id),
                        pos: 0.0,
                        distance_travelled: 0.0,
                        route: *id,
                    };

                    start_station_mut.train = Some(curr_train_id);
                    if t >= 0 {
                        start_station_mut
                            .arrival_times
                            .entry(*id)
                            .or_default()
                            .push(t as f64);
                    }
                    self.trains.insert(curr_train_id, train);
                    train_to_route.insert(curr_train_id, *id);
                    self.curr_train_counts[id.0 as usize] += 1;
                }
            }

            println!("Iteration: {t}, train count: {}", self.trains.len());

            let mut curr_train_positions = Vec::new();
            for (id, train) in &self.trains {
                curr_train_positions.push(TrainPosition {
                    id: *id,
                    curr_section: train.curr_section,
                    pos: train.pos,
                    distance_travelled: train.distance_travelled,
                })
            }
            if t >= 0 {
                train_positions.push(TrainPositions {
                    time: t as u32,
                    trains: curr_train_positions,
                });
            }

            t += 1;
        }

        SimulationResults {
            train_positions,
            train_to_route,
            station_statistics: self
                .stations
                .into_iter()
                .map(|(id, s)| {
                    (
                        id,
                        StationStatistic {
                            arrival_times: s.arrival_times,
                        },
                    )
                })
                .collect(),
        }
    }

    // This is mostly a copy paste of the run function right now.
    // TODO figure out how to consolidate code with run
    pub fn schedule_trains<'a>(
        &mut self,
        iterations: i32,
        desired_frequencies: &Frequencies,
        z3_context: &'a z3::Context,
        conflicts: &[z3::ast::Bool]
    ) -> Option<(SimulationResults, Vec<z3::ast::Bool<'a>>)> {

        let z3_solver = z3::Solver::new(z3_context);
        
        let mut routes: Vec<_> = self.routes.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
        let mut rng = StdRng::seed_from_u64(5050);

        let mut frequencies = Vec::with_capacity(desired_frequencies.len());
        for period in desired_frequencies {
            let mut map = HashMap::with_capacity(period.len());
            for (route, frequency) in period.iter() {
                map.insert(route.clone(), frequency.get());
            }
            frequencies.push(map);
        }

        // initialize ground rules for all departure variables. Specifically,
        // r_i+1 > r_i, and depending on frequencies set time bounds:
        // r_0 >= 0 and r_0 < SCHEDULE_GRANULARITY must always be true

        for (id, route) in self.routes.iter() {
            let mut start_time = 0;
            let mut curr_idx = 0;
            for freq in &frequencies {
                let end_time = start_time + SCHEDULE_GRANULARITY;

                for i in 0..freq[&route.name] {
                    let curr_train = TrainId {
                        route_idx: id.0,
                        count: (i + curr_idx) as u32,
                    }
                    .to_z3_departure(&z3_context);
                    let next_train = TrainId {
                        route_idx: id.0,
                        count: (i + curr_idx + 1) as u32,
                    }
                    .to_z3_departure(&z3_context);

                    z3_solver.assert(
                        &z3::ast::Int::add(
                            &z3_context,
                            &[
                                &curr_train,
                                &z3::ast::Int::from_u64(&z3_context, MIN_TRAIN_DISTANCE as u64),
                            ],
                        )
                        .le(&next_train),
                    );
                    z3_solver
                        .assert(&curr_train.ge(&z3::ast::Int::from_i64(&z3_context, start_time)));
                    z3_solver
                        .assert(&curr_train.lt(&z3::ast::Int::from_i64(&z3_context, end_time)));
                }

                start_time = end_time;
                curr_idx += freq[&route.name];
            }
        }
        
        for conflict in conflicts {
            z3_solver.assert(conflict);
        }

        let mut train_to_route = HashMap::new();
        let traversal_order = self.traversal_order.clone();

        let mut train_positions = Vec::new();

        let mut t = 0;

        let mut train_scheduled_at = HashMap::new();
        let mut states = Vec::with_capacity(iterations as usize);

        let mut new_conflicts = Vec::new();

        'iteration: while t < iterations {
            states.push((self.clone(), frequencies.clone()));
            assert_eq!(states.len(), t as usize + 1);
            z3_solver.push();

            for track_station in &traversal_order {
                match *track_station {
                    TrackStationId::Station(station) => {
                        self.station_to_track(station, TIME_STEP);
                    }
                    TrackStationId::Track(track) => {
                        let mut i = 0;
                        let mut last_train_pos = f64::INFINITY;
                        let next_station_id = self.subway_map.edge_endpoints(track).unwrap().1;
                        while i < self.tracks.get_mut(&track).unwrap().trains.len() {
                            let track_mut = self.tracks.get_mut(&track).unwrap();
                            if self.stations[&next_station_id].train.is_some() {
                                last_train_pos = f64_min(
                                    f64_max(track_mut.length as f64 - MIN_TRAIN_DISTANCE, 0.0),
                                    last_train_pos,
                                );
                            }
                            let curr_train_id = track_mut.trains[i];
                            let curr_train_mut = self.trains.get_mut(&curr_train_id).unwrap();
                            let mut time_left = TIME_STEP;
                            
                            if last_train_pos - MIN_TRAIN_DISTANCE <  curr_train_mut.pos + time_left {
                                // MERGE CONFLICT
                                
                                let conflicting_train = if track_mut.trains.len() > i + 1 {
                                    track_mut.trains[i+1]
                                } else {
                                    self.stations[&next_station_id].train.unwrap()
                                };
                                
                                let scheduled_at = min(train_scheduled_at[&curr_train_id], train_scheduled_at[&conflicting_train]);
                                t = scheduled_at;
                                let num_states_removed =
                                    states.len() - scheduled_at as usize;
                                states.drain(scheduled_at as usize + 1..);
                                let prev_state = states.pop().unwrap();

                                *self = prev_state.0;
                                frequencies = prev_state.1;

                                train_positions.retain(|p: &TrainPositions| {
                                    (p.time as i32) < scheduled_at
                                });

                                // restore solver state to the iteration we're returning to
                                z3_solver.pop(num_states_removed as u32);

                                // TODO quadratic performance, FIXME
                                for assertion in &new_conflicts {
                                    z3_solver.assert(assertion);
                                }
                                // encode conflict
                                let conflicting_train_scheduled_at = conflicting_train
                                    .to_z3_departure(&z3_context)
                                    ._eq(&z3::ast::Int::from_i64(
                                        &z3_context,
                                        train_scheduled_at[&conflicting_train] as i64,
                                    ));
                                let assertion = conflicting_train_scheduled_at.implies(
                                    &curr_train_id
                                        .to_z3_departure(&z3_context)
                                        ._eq(&z3::ast::Int::from_i64(
                                            &z3_context,
                                            train_scheduled_at[&curr_train_id] as i64,
                                        ))
                                        .not(),
                                );
                                z3_solver.assert(&assertion);
                                new_conflicts.push(assertion);

                                continue 'iteration;

                            }

                            let travel_distance = f64_min(
                                f64_min(
                                    time_left,
                                    f64_max(track_mut.length as f64 - curr_train_mut.pos, 0.0),
                                ),
                                f64_max(
                                    if curr_train_mut.pos + MIN_TRAIN_DISTANCE >= last_train_pos {
                                        last_train_pos - MIN_TRAIN_DISTANCE - curr_train_mut.pos
                                    } else {
                                        last_train_pos - curr_train_mut.pos
                                    },
                                    0.0,
                                ),
                            );
                            
                            curr_train_mut.pos += travel_distance;
                            time_left -= travel_distance;
                            // we're done with the current track, and need to move into the station
                            if curr_train_mut.pos >= track_mut.length as f64
                                && self.stations[&next_station_id].train.is_none()
                            {
                                match self.stations[&next_station_id].train {
                                    None => {
                                        debug_assert_eq!(i, 0);
                                        track_mut.trains.pop_front();
                                        debug_assert!(
                                            self.stations
                                                .get_mut(&next_station_id)
                                                .unwrap()
                                                .train
                                                .is_none(),
                                            "travel distance is {travel_distance}"
                                        );
                                        let next_station_mut =
                                            self.stations.get_mut(&next_station_id).unwrap();
                                        next_station_mut.train = Some(curr_train_id);
                                        if t >= 0 {
                                            next_station_mut
                                                .arrival_times
                                                .entry(curr_train_mut.route)
                                                .or_default()
                                                .push(t as f64 + travel_distance);
                                        }

                                        curr_train_mut.distance_travelled +=
                                            self.tracks[&track].length as f64;

                                        curr_train_mut.curr_section =
                                            TrackStationId::Station(next_station_id);
                                        curr_train_mut.pos = 0.0;

                                        self.station_to_track(next_station_id, time_left);
                                        // TODO: handle the fact that some station time may be wasted when the train could keep moving
                                        // potentially some of this spaghetti code needs to get factored out into
                                        // a separate function, tbd
                                    }
                                    Some(_) => {
                                        panic!("wait how are we getting here?");
                                    }
                                };
                            } else {
                                last_train_pos = curr_train_mut.pos;
                                i += 1;
                            }
                        }
                    }
                }
            }

            routes.shuffle(&mut rng);
            for (id, route) in &routes {
                let start_station_mut = self.stations.get_mut(&route.start_station).unwrap();
                // TODO: do I need to handle the case where this is not true?
                let curr_train_id = TrainId {
                    route_idx: id.0,
                    count: self.curr_train_counts[id.0 as usize],
                };

                if frequencies[(t as i64 / SCHEDULE_GRANULARITY) as usize][&route.name] == 0 {
                    continue;
                }

                if start_station_mut.train.is_none() {
                    let train = Train {
                        id: curr_train_id,
                        curr_section: TrackStationId::Station(start_station_mut.id),
                        pos: 0.0,
                        distance_travelled: 0.0,
                        route: *id,
                    };

                    // logic to handle when trying to schedule trains:
                    // - should we wait if there's currently a train too close on the directly proceeding
                    //   track? right now, will say no, but otherwise this would be the first check
                    // - check if we've failed frequency requirements for last bin. if yes, report fail
                    // - add an assertion to solver saying current train departure = time. if we get a sat
                    //   model, proceed. otherwise, don't schedule a train
                    // - the difficult thing to figure out is: what assertions can we keep permanently,
                    //   and what can we get rid of? would be nice if we could maintain a list of permanent assumptions
                    //   but otherwise do pop and push logic. a nice thing to assert then pop is depart_var = (or >=) curr time
                    //   however when backtracking this could of course get invalidated, or could it? think about this

                    let curr_train_z3 = curr_train_id.to_z3_departure(&z3_context);
                    let curr_time_z3 = z3::ast::Int::from_i64(&z3_context, t.into());

                    z3_solver.push();
                    if z3_solver.check_assumptions(&[curr_train_z3.ge(&curr_time_z3)])
                        != z3::SatResult::Sat
                    {
                        // TODO attempt to allow a merge later? requires complex pruning of assertions
                        return None;
                    }
                    let z3_departure_equality = curr_train_z3._eq(&curr_time_z3);
                    z3_solver.assert(&z3_departure_equality);
                    if z3_solver.check_assumptions(&[]) != z3::SatResult::Sat {
                        z3_solver.pop(1);
                        continue;
                    }

                    z3_solver.pop(1);
                    z3_solver.assert(&z3_departure_equality);

                    *frequencies[(t as i64 / SCHEDULE_GRANULARITY) as usize]
                        .get_mut(&route.name)
                        .unwrap() -= 1;

                    start_station_mut.train = Some(curr_train_id);
                    if t >= 0 {
                        start_station_mut
                            .arrival_times
                            .entry(*id)
                            .or_default()
                            .push(t as f64);
                    }
                    self.trains.insert(curr_train_id, train);
                    train_to_route.insert(curr_train_id, *id);
                    train_scheduled_at.insert(curr_train_id, t);
                    self.curr_train_counts[id.0 as usize] += 1;
                }
            }

            let mut curr_train_positions = Vec::new();
            for (id, train) in &self.trains {
                curr_train_positions.push(TrainPosition {
                    id: *id,
                    curr_section: train.curr_section,
                    pos: train.pos,
                    distance_travelled: train.distance_travelled,
                })
            }
            if t >= 0 {
                train_positions.push(TrainPositions {
                    time: t as u32,
                    trains: curr_train_positions,
                });
            }

            t += 1;
        }

        Some((SimulationResults {
            train_positions,
            train_to_route,
            station_statistics: self
                .stations
                .iter()
                .map(|(id, s)| {
                    (
                        *id,
                        StationStatistic {
                            arrival_times: s.arrival_times.clone(),
                        },
                    )
                })
                .collect(),
        }, new_conflicts))
    }
}

/// Gets all nodes that have no out edges
fn terminal_nodes(graph: &SubwayMap) -> Vec<NodeIndex> {
    graph
        .node_indices()
        .filter(|&node| {
            graph
                .edges_directed(node, Direction::Outgoing)
                .filter(|e| e.weight().ty == EdgeType::Track)
                .count()
                == 0
        })
        .collect()
}

pub struct Trip {
    pub start: NodeIndex,
    pub end: NodeIndex,
    pub count: usize,
}

pub type TripData = HashMap<i64, Vec<Trip>>;
// A map from route ids to scheduled times for the trains to depart
type Schedule = HashMap<String, Vec<i64>>;

pub const SCHEDULE_GRANULARITY: i64 = 12;
pub const SCHEDULE_PERIOD: i64 = 120;

type Frequencies = Vec<HashMap<String, Cell<i64>>>;

pub fn optimize(
    subway_map: SubwayMap,
    routes: Vec<Route>,
    trip_data: &TripData,
    shortest_paths: &HashMap<(NodeIndex, NodeIndex), Vec<Vec<PathSegment>>>,
) -> (Schedule, Option<SimulationResults>) {
    let mut frequencies: Frequencies =
        Vec::with_capacity((SCHEDULE_PERIOD / SCHEDULE_GRANULARITY) as usize);
    for _ in 0..(SCHEDULE_PERIOD / SCHEDULE_GRANULARITY) {
        let mut map = HashMap::with_capacity(routes.len());
        for route in &routes {
            map.insert(route.name.clone(), Cell::new(1));
        }
        frequencies.push(map);
    }
    // blacklisted time + route combos that should no longer be considered because they make performance worse
    let mut blacklisted_fragments = HashSet::new();

    let mut curr_cost = f64::MAX;

    let mut curr_schedule = Schedule::new();
    for route in &routes {
        curr_schedule.insert(
            route.name.clone(),
            vec![1; (SCHEDULE_PERIOD / SCHEDULE_GRANULARITY) as usize],
        );
    }

    let mut curr_simulation_results = None;

    let mut search_map = SearchMap::generate(&subway_map, &routes);

    let mut routes_vec = Vec::with_capacity(routes.len());
    for route in &routes {
        routes_vec.push(route.clone());
    }
    let mut simulator = Simulator::new(subway_map, routes_vec);

    // use z3 SMT to calculate train position bounds
    // details: each train is scheduled to depart at an integer time.
    // the ith train for route r has a variable called r_i
    // we get train departure times from z3. Any time we observe a conflict, we
    // add a rule probibiting the cause of the conflict, then jump back in time before the
    // conflict occurred.
    let z3_config = z3::Config::new();
    let z3_context = z3::Context::new(&z3_config);
    
    // z3 conflict clauses learned over time
    let mut conflicts = Vec::new();

    loop {
        let mut best_fragment = None;
        let mut lowest_cost = f64::INFINITY;

        for (time, route_frequencies) in frequencies.iter().enumerate() {
            for (id, frequency) in route_frequencies.iter() {
                if blacklisted_fragments.contains(&(time, id.clone()))
                    || frequency.get() >= SCHEDULE_GRANULARITY
                {
                    continue;
                }
                frequency.set(frequency.get() + 1);
                // calculate cost if frequency goes up by increment of 1
                let estimated_cost = calculate_costs(
                    &mut search_map,
                    &frequencies,
                    &routes,
                    trip_data,
                    shortest_paths,
                );
                if estimated_cost < lowest_cost {
                    lowest_cost = estimated_cost;
                    best_fragment = Some((time, id.clone()));
                }
                frequency.set(frequency.get() - 1);
            }
        }

        let best_fragment = match best_fragment {
            Some(best_fragment) => best_fragment,
            None => {
                println!("Found with cost: {curr_cost}");
                return (curr_schedule, curr_simulation_results);
            }
        };

        println!("Found best fragment: {:?}", best_fragment);

        *frequencies[best_fragment.0]
            .get_mut(&best_fragment.1)
            .unwrap()
            .get_mut() += 1;
        let simulation_results =
            simulator.schedule_trains(SCHEDULE_PERIOD as i32, &frequencies, &z3_context, &conflicts);
        simulator.reset();
        
        // TODO should we get an actual cost estimate here?
        let cost = if let Some((simulation_results, mut new_conflicts)) = simulation_results {
            curr_simulation_results = Some(simulation_results);
            conflicts.append(&mut new_conflicts);
            lowest_cost
        } else {
            f64::INFINITY
        };

        if cost < curr_cost {
            curr_cost = cost;
            curr_schedule.get_mut(&best_fragment.1).unwrap()[best_fragment.0] += 1;
        } else {
            *frequencies[best_fragment.0]
                .get_mut(&best_fragment.1)
                .unwrap()
                .get_mut() -= 1;
            blacklisted_fragments.insert(best_fragment);
        }
    }
}

// for search, modify graph? what we could do is duplicate each node and edge per route. then if
// a route is no longer helpful for us, we dip

// generate modified map for use in shortest routes search
// creates a map where each route has its own nodes and edges; if two routes share the same
// nodes and edges, walk nodes of cost 0 connect them

#[derive(PartialEq, Eq, Hash)]
pub struct SearchNode {
    route: String,
    old_node: NodeIndex,
}

#[derive(Copy, Clone)]
pub struct SearchEdge {
    pub ty: EdgeType,
    pub weight: u16,
    pub disabled: bool,
}

impl SearchEdge {
    fn cost(self) -> u16 {
        if self.disabled {
            1000
        } else {
            self.weight
        }
    }
}

impl From<Edge> for SearchEdge {
    fn from(edge: Edge) -> Self {
        SearchEdge {
            ty: edge.ty,
            weight: edge.weight,
            disabled: false,
        }
    }
}

pub type SearchGraph = Graph<SearchNode, SearchEdge>;

pub struct SearchMap {
    map: SearchGraph,
    old_to_new_nodes: HashMap<NodeIndex, Vec<NodeIndex>>,
    old_to_new_edges: HashMap<EdgeIndex, Vec<EdgeIndex>>,
    new_to_old_edges: HashMap<EdgeIndex, EdgeIndex>,
}

impl SearchMap {
    // Maps a subway map to a form that is more amenable to searching for best routes.
    // Each route is given its own nodes and edges, but if multiple routes share nodes in the actual
    // map, they will be connected with walk edges.
    pub fn generate(subway_map: &SubwayMap, routes: &[Route]) -> Self {
        let mut search_map = SearchGraph::new();
        let mut old_to_new_nodes = HashMap::new();
        let mut old_to_new_edges = HashMap::new();
        let mut new_to_old_edges = HashMap::new();

        let mut route_old_to_new_nodes = HashMap::new();

        // For each route create nodes and edges for it
        for route in routes {
            let mut create_node =
                |old_node: NodeIndex, search_map: &mut SearchGraph| -> NodeIndex {
                    match route_old_to_new_nodes.get(&(&route.name, old_node)) {
                        Some(node) => *node,
                        None => {
                            let new_nodes = old_to_new_nodes.entry(old_node).or_insert(Vec::new());
                            let route_node = search_map.add_node(SearchNode {
                                route: route.name.clone(),
                                old_node,
                            });
                            new_nodes.push(route_node);
                            route_old_to_new_nodes.insert((&route.name, old_node), route_node);
                            route_node
                        }
                    }
                };

            for edge in route.station_to.values() {
                let (start, end) = subway_map.edge_endpoints(*edge).unwrap();
                let new_start_node = create_node(start, &mut search_map);
                let new_end_node = create_node(end, &mut search_map);
                let new_edge =
                    search_map.add_edge(new_start_node, new_end_node, subway_map[*edge].into());
                old_to_new_edges
                    .entry(*edge)
                    .or_insert(Vec::new())
                    .push(new_edge);
                new_to_old_edges.insert(new_edge, *edge);
            }
        }

        // Connect virtual nodes that correspond to the same station together with walk edges. This
        // represents the transfer necessary to move between routes
        for related_nodes in old_to_new_nodes.values() {
            for i in 0..related_nodes.len() - 1 {
                for j in i + 1..related_nodes.len() {
                    search_map.add_edge(
                        related_nodes[i],
                        related_nodes[j],
                        SearchEdge {
                            ty: crate::EdgeType::Walk,
                            weight: 1,
                            disabled: false,
                        },
                    );
                }
            }
        }

        // Create corresponding walk edges for those found on the original graph
        for edge in subway_map.edge_references() {
            if let EdgeType::Walk = edge.weight().ty {
                let node1 = edge.source();
                let node2 = edge.target();
                // ignore cases where no routes in the network use a station node
                if !(old_to_new_nodes.contains_key(&node1) && old_to_new_nodes.contains_key(&node2))
                {
                    continue;
                }
                let new_nodes1 = &old_to_new_nodes[&node1];
                let new_nodes2 = &old_to_new_nodes[&node2];
                // TODO: this causes quadratic blowup of walk edge numbers which is sometimes excessive.
                // Is this too aggressive? Can we just create one edge instead?
                for node1 in new_nodes1 {
                    for node2 in new_nodes2 {
                        search_map.add_edge(*node1, *node2, (*edge.weight()).into());
                    }
                }
            }
        }

        SearchMap {
            map: search_map,
            old_to_new_nodes,
            old_to_new_edges,
            new_to_old_edges,
        }
    }
}

// Finds the k shortest paths between the start and end nodes.
// Note that start and end must be old nodes.
// things to think about here
// - shortest paths need to take routes into account: even if track is physically connected,
// it doesn't mean there's a route that actually is able to go through the entire physical connection
// - if part of route a to d goes through b to c, we can take any route that goes on those
// and add their frequency to reduce travel times
// NOTE: this tactic only works if there aren't cases where routes X and Y share tracks, diverge,
// then merge back together. this can be problematic as the section where they diverge may have varying
// speeds, eg express vs local service. This can be seen with D and rush hour B service, where the lines
// reconnect in the Bronx. For now the B service will always terminate earlier than this point.
pub fn shortest_paths(
    start: NodeIndex,
    end: NodeIndex,
    search_map: &mut SearchMap,
    mut k: usize,
) -> Vec<Vec<PathSegment>> {
    assert!(k >= 1);
    let valid_end_nodes: HashSet<_> = search_map.old_to_new_nodes[&end]
        .clone()
        .into_iter()
        .collect();

    let start_nodes = &search_map.old_to_new_nodes[&start];

    // create virtual node + edges to represent start of search
    // note that route and old nodeindex are invalid for virtual node
    let virtual_start_node = search_map.map.add_node(SearchNode {
        route: String::new(),
        old_node: NodeIndex::new(0),
    });

    for start_node in start_nodes {
        search_map.map.add_edge(
            virtual_start_node,
            *start_node,
            SearchEdge {
                ty: EdgeType::Walk,
                weight: 0,
                disabled: false,
            },
        );
    }

    // TODO: should we consider route frequencies in this calculation? pass that data here if yes
    let (costs, destination) = dijkstra(
        &search_map.map,
        virtual_start_node,
        &valid_end_nodes,
        |edge| edge.weight().cost(),
    );
    let destination = match destination {
        Terminated::Exhaustive => {
            return Vec::new();
        }
        Terminated::At(destination) => destination,
    };

    let path = search_to_path(search_map, &costs, destination);
    k -= 1;

    let mut paths = Vec::new();

    for (i, segment) in path.iter().enumerate() {
        if k == 0 {
            break;
        }

        let mut disabled_edges = Vec::new();

        // If the segment is the last one in the path, then disabling walk edges at that node won't
        // work. Instead, disconnect the route edge to that node. Do this for all routes in the segment.
        if i == path.len() - 1 {
            let original_end_node = search_map.map[segment.end_node].old_node;
            for new_node in &search_map.old_to_new_nodes[&original_end_node] {
                if segment.routes.contains(&search_map.map[*new_node].route) {
                    for edge in search_map
                        .map
                        .edges_directed(*new_node, Direction::Incoming)
                    {
                        disabled_edges.push(edge.id());
                    }
                }
            }
        } else {
            for neighbor in search_map.map.neighbors_undirected(segment.end_node) {
                for edge in search_map.map.edges_connecting(segment.end_node, neighbor) {
                    if edge.weight().ty == EdgeType::Walk {
                        disabled_edges.push(edge.id());
                    }
                }
            }
        }

        for edge in &disabled_edges {
            search_map.map[*edge].disabled = true;
        }

        let (costs, destination) = dijkstra(
            &search_map.map,
            virtual_start_node,
            &valid_end_nodes,
            |edge| edge.weight().cost(),
        );

        for edge in &disabled_edges {
            search_map.map[*edge].disabled = false;
        }

        let destination = match destination {
            Terminated::Exhaustive => continue,
            Terminated::At(destination) => destination,
        };
        let path = search_to_path(search_map, &costs, destination);
        paths.push(path);

        k -= 1;
    }
    paths.push(path);

    search_map.map.remove_node(virtual_start_node);

    paths
}

#[derive(Debug)]
pub struct PathSegment {
    routes: HashSet<String>,
    cost: u16,
    start_node: NodeIndex,
    end_node: NodeIndex,
    edge_to_next: Option<EdgeIndex>,
}

fn search_to_path(
    search_map: &SearchMap,
    costs: &HashMap<NodeIndex, (u16, Option<EdgeIndex>)>,
    destination: NodeIndex,
) -> Vec<PathSegment> {
    let mut curr_edge = costs[&destination].1.unwrap();
    let mut paths = Vec::new();

    // These are updated as we're traversing a current route, and used to get the final route data
    let mut curr_end_node = search_map.map.edge_endpoints(curr_edge).unwrap().0;
    let mut curr_start_node = curr_end_node;
    let mut curr_cost = 0;
    let mut currently_in_segment = false;
    loop {
        let source_node = search_map.map.edge_endpoints(curr_edge).unwrap().0;
        let edge_ref = &search_map.map[curr_edge];
        if edge_ref.ty == EdgeType::Walk {
            if currently_in_segment {
                let old_start_node = search_map.map[curr_start_node].old_node;
                let start_routes: HashSet<_> = search_map.old_to_new_nodes[&old_start_node]
                    .iter()
                    .map(|node| search_map.map[*node].route.clone())
                    .collect();
                let old_end_node = search_map.map[curr_end_node].old_node;
                let end_routes: HashSet<_> = search_map.old_to_new_nodes[&old_end_node]
                    .iter()
                    .map(|node| search_map.map[*node].route.clone())
                    .collect();
                let routes = HashSet::from_iter(start_routes.intersection(&end_routes).cloned());
                paths.push(PathSegment {
                    routes,
                    cost: curr_cost,
                    start_node: curr_start_node,
                    end_node: curr_end_node,
                    edge_to_next: Some(curr_edge),
                });
                currently_in_segment = false;
            }
            curr_cost = 0;
        } else {
            if !currently_in_segment {
                curr_end_node = search_map.map.edge_endpoints(curr_edge).unwrap().1;
                currently_in_segment = true;
            }
            curr_start_node = search_map.map.edge_endpoints(curr_edge).unwrap().0;
            curr_cost += edge_ref.weight;
        }

        curr_edge = match costs[&source_node].1 {
            Some(curr_edge) => curr_edge,
            None => {
                break;
            }
        };
    }

    paths.reverse();
    paths
}

const WALK_MULTIPLIER: f64 = 2.5;
const WAIT_MULTIPLIER: f64 = 2.1;

fn calculate_time_to(search_map: &SearchMap, mut node: NodeIndex) -> f64 {
    let mut time = 0.;
    loop {
        let mut new_node = node;
        for edge in search_map.map.edges_directed(node, Direction::Incoming) {
            if edge.weight().ty == EdgeType::Track {
                time += edge.weight().weight as f64 + STATION_DWELL_TIME;
                new_node = edge.source();
                break;
            }
        }

        if new_node != node {
            node = new_node;
        } else {
            return time;
        }
    }
}

fn calculate_costs(
    search_map: &mut SearchMap,
    frequencies: &[HashMap<String, Cell<i64>>],
    routes: &[Route],
    trip_data: &TripData,
    shortest_paths: &HashMap<(NodeIndex, NodeIndex), Vec<Vec<PathSegment>>>,
) -> f64 {
    let mut total_cost = 0.;

    let mut time_to_cache = HashMap::with_capacity(routes.len());
    for route in routes {
        time_to_cache.insert(route.name.clone(), HashMap::new());
    }

    for (time, trips) in trip_data.iter() {
        for trip in trips {
            let paths = &shortest_paths[&(trip.start, trip.end)];
            assert!(!paths.is_empty());
            let mut lowest_cost = f64::INFINITY;
            'path: for path in paths.iter() {
                let mut curr_time = *time as f64;
                let mut cost = 0.;
                for segment in path {
                    let mut total_frequency = 0;
                    for route in &segment.routes {
                        let time_to_start = *time_to_cache
                            .get_mut(route)
                            .unwrap()
                            .entry(segment.start_node)
                            .or_insert_with(|| calculate_time_to(&search_map, segment.start_node));
                        let curr_schedule =
                            (curr_time as i64 - time_to_start as i64) / SCHEDULE_GRANULARITY;
                        if curr_schedule < 0 || curr_schedule >= frequencies.len() as i64 {
                            // if the journey runs overtime stop considering subsequent segments
                            continue;
                        }
                        total_frequency += frequencies[curr_schedule as usize][route].get();
                    }

                    if total_frequency == 0 {
                        continue 'path;
                    }

                    let wait =
                        SCHEDULE_GRANULARITY as f64 / total_frequency as f64 * WAIT_MULTIPLIER;
                    let total_segment_cost = segment.cost as f64 + wait;
                    cost += total_segment_cost;
                    curr_time += total_segment_cost;
                    if let Some(edge_idx) = segment.edge_to_next {
                        let walk_time = search_map
                            .map
                            .edge_weight(edge_idx)
                            .map(|e| e.weight)
                            .unwrap_or_default() as f64
                            * WALK_MULTIPLIER;
                        cost += walk_time;
                        curr_time += walk_time;
                    }
                }
                lowest_cost = lowest_cost.min(cost);
            }
            if lowest_cost < f64::INFINITY {
                total_cost += lowest_cost * trip.count as f64;
            }
        }
    }
    total_cost
}
