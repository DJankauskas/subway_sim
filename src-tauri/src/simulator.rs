use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use petgraph::Graph;
use serde::Serialize;

use crate::shortest_path::{dijkstra, Terminated};
use crate::{Edge, EdgeType};

pub type SubwayMap = Graph<String, Edge>;
pub type StationId = NodeIndex<u32>;
pub type TrackId = EdgeIndex;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TrainId(pub u32);
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
    curr_train_id: TrainId,
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
    *[a, b].iter().min_by(|a, b| a.total_cmp(b)).unwrap()
}

fn f64_max(a: f64, b: f64) -> f64 {
    *[a, b].iter().max_by(|a, b| a.total_cmp(b)).unwrap()
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

        while let Some(track_station) = queue.pop_front() {
            if visited.contains(&track_station) {
                continue;
            };
            visited.insert(track_station);
            traversal_order.push(track_station);
            match track_station {
                TrackStationId::Track(track) => {
                    let (source, _) = subway_map.edge_endpoints(track).unwrap();
                    queue.push_back(TrackStationId::Station(source));
                }
                TrackStationId::Station(station) => subway_map
                    .edges_directed(station, Direction::Incoming)
                    .for_each(|track| queue.push_back(TrackStationId::Track(track.id()))),
            }
        }

        let routes = routes
            .into_iter()
            .enumerate()
            .map(|(i, route)| (RouteId(i as u32), route))
            .collect();

        Self {
            subway_map,
            routes,
            trains: HashMap::new(),
            curr_train_id: TrainId(0),
            stations,
            tracks,
            traversal_order,
        }
    }

    fn station_to_track(&mut self, station: StationId, mut time_left: f64) {
        if let Some(train) = &self.stations[&station].train {
            let train = *train;
            let train_mut = self.trains.get_mut(&train).unwrap();
            let distance_travelled = f64_max(STATION_DWELL_TIME - train_mut.pos, 0.0);
            train_mut.pos += distance_travelled;
            time_left -= distance_travelled;
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
                    let pos_move = f64_min(time_left, last_train_pos - MIN_TRAIN_DISTANCE);
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
                                last_train_pos =
                                    f64_max(track_mut.length as f64 - MIN_TRAIN_DISTANCE, 0.0);
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

            // TODO: replace with actual scheduling data
            for (id, route) in &self.routes {
                if (t - route.offset as i32) % (frequency as i32) != 0 {
                    continue;
                }
                let start_station_mut = self.stations.get_mut(&route.start_station).unwrap();
                // TODO: do I need to handle the case where this is not true?
                if start_station_mut.train.is_none() {
                    let train = Train {
                        id: self.curr_train_id,
                        curr_section: TrackStationId::Station(start_station_mut.id),
                        pos: 0.0,
                        distance_travelled: 0.0,
                        route: *id,
                    };

                    start_station_mut.train = Some(self.curr_train_id);
                    if t >= 0 {
                        start_station_mut
                            .arrival_times
                            .entry(*id)
                            .or_default()
                            .push(t as f64);
                    }
                    self.trains.insert(self.curr_train_id, train);
                    train_to_route.insert(self.curr_train_id, *id);
                    self.curr_train_id = TrainId(self.curr_train_id.0 + 1);
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
    pub fn hyper_hacky_schedule_trains(mut self, iterations: i32, frequency: u64, desired_frequencies: &Frequencies) -> SimulationResults {
        let mut train_to_route = HashMap::new();
        let traversal_order = self.traversal_order.clone();
        println!("{:?}", traversal_order);
        for route in self.routes.values() {
            println!("{:?}", route.start_station);
        }

        let mut train_positions = Vec::new();

        let mut t = 0;
        
        let mut train_scheduled_at = HashMap::new();
        let mut states = Vec::with_capacity(iterations as usize);

        'iteration:
        while t < iterations {
            states.push(self.clone());
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
                                last_train_pos =
                                    f64_max(track_mut.length as f64 - MIN_TRAIN_DISTANCE, 0.0);
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
                                if self.stations[&next_station_id].train.is_none() {
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
                                    // MERGE CONFLICT
                                    // todo random chance of allowing the merge conflict instead, and 
                                    // continuing
                                    let scheduled_at = train_scheduled_at[&curr_train_id];
                                    t = scheduled_at;
                                    states.drain(scheduled_at as usize+1..);
                                    self = states.pop().unwrap();
                                    train_positions.retain(|p: &TrainPositions| (p.time as i32) < scheduled_at);
                                    break 'iteration;
                                }

                            } else {
                                last_train_pos = curr_train_mut.pos;
                                i += 1;
                            }
                        }
                    }
                }
            }

            // TODO: replace with actual scheduling data
            for (id, route) in &self.routes {
                if (t - route.offset as i32) % (frequency as i32) != 0 {
                    continue;
                }
                let start_station_mut = self.stations.get_mut(&route.start_station).unwrap();
                // TODO: do I need to handle the case where this is not true?
                if start_station_mut.train.is_none() {
                    let train = Train {
                        id: self.curr_train_id,
                        curr_section: TrackStationId::Station(start_station_mut.id),
                        pos: 0.0,
                        distance_travelled: 0.0,
                        route: *id,
                    };

                    start_station_mut.train = Some(self.curr_train_id);
                    if t >= 0 {
                        start_station_mut
                            .arrival_times
                            .entry(*id)
                            .or_default()
                            .push(t as f64);
                    }
                    self.trains.insert(self.curr_train_id, train);
                    train_to_route.insert(self.curr_train_id, *id);
                    train_scheduled_at.insert(self.curr_train_id, t);
                    self.curr_train_id = TrainId(self.curr_train_id.0 + 1);
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
}

/// Gets all nodes that have no out edges
/// TODO: customize so it ignores walk edges
fn terminal_nodes(graph: &SubwayMap) -> Vec<NodeIndex> {
    graph
        .node_indices()
        .filter(|&node| graph.neighbors_directed(node, Direction::Outgoing).count() == 0)
        .collect()
}

struct Trip {
    start: NodeIndex,
    end: NodeIndex,
    count: usize,
}

// All desired trips at a given time. TODO is this the nicest data layout? 
type TripData = HashMap<i64, Vec<Trip>>;
// A map from route ids to scheduled times for the trains to depart
type Schedule = HashMap<String, Vec<i64>>;

const SCHEDULE_GRANULARITY: i64 = 30;
const SCHEDULE_PERIOD: i64 = 240;

type Frequencies = Vec<HashMap<String, Cell<i64>>>;


fn optimize(subway_map: SubwayMap, routes: HashMap<String, Route>, trip_data: &TripData) -> Schedule {
    let mut frequencies: Frequencies  = Vec::with_capacity((SCHEDULE_PERIOD / SCHEDULE_GRANULARITY) as usize);
    // blacklisted time + route combos that should no longer be considered because they make performance worse
    let mut blacklisted_fragments = HashSet::new();
    
    let mut curr_cost = f64::INFINITY;
    let mut curr_schedule = Schedule::new();
    
    let mut search_map = generate_shortest_path_search_map(&subway_map, &routes);
    
    loop {
        let mut best_fragment = None;
        let mut lowest_cost = f32::INFINITY;
        
        for (time, route_frequencies) in frequencies.iter().enumerate() {
            for (id, frequency) in route_frequencies.iter() {
                if blacklisted_fragments.contains(&(time, id.clone())) || frequency.get() >= SCHEDULE_GRANULARITY {
                    continue;
                }
                frequency.set(frequency.get() + 1);
                // calculate cost if frequency goes up by increment of 1
                // TODO replace dummy cost value
                let estimated_cost = calculate_costs(&mut search_map, &frequencies, &routes, trip_data);
                if estimated_cost < lowest_cost {
                    lowest_cost = estimated_cost;
                    best_fragment = Some((time, id.clone()));
                }
                frequency.set(frequency.get() - 1);
            }
        }
        
        let best_fragment = match best_fragment {
            Some(best_fragment) => best_fragment,
            None => return curr_schedule,
        };
        
        *frequencies[best_fragment.0].get_mut(&best_fragment.1).unwrap().get_mut() += 1;
        // run_simulation
        // todo get correct cost
        let cost = 100.0;
        if cost < curr_cost {
            curr_cost = cost;
        } else {
            *frequencies[best_fragment.0].get_mut(&best_fragment.1).unwrap().get_mut() -= 1;
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

pub type SearchGraph = Graph<SearchNode, Edge>;

pub struct SearchMap {
    map: SearchGraph,
    old_to_new_nodes: HashMap<NodeIndex, Vec<NodeIndex>>,
    old_to_new_edges: HashMap<EdgeIndex, Vec<EdgeIndex>>,
    new_to_old_edges: HashMap<EdgeIndex, EdgeIndex>,
}

// Maps a subway map to a form that is more amenable to searching for best routes.
// Each route is given its own nodes and edges, but if multiple routes share nodes in the actual 
// map, they will be connected with walk edges. 
pub fn generate_shortest_path_search_map(subway_map: &SubwayMap, routes: &HashMap<String, Route>) -> SearchMap {
    let mut search_map  = SearchGraph::new();
    let mut old_to_new_nodes = HashMap::new();
    let mut old_to_new_edges = HashMap::new();
    let mut new_to_old_edges = HashMap::new();
    
    let mut route_old_to_new_nodes = HashMap::new();
    
    // For each route create nodes and edges for it
    for (key, route) in routes.iter() {
        let mut create_node = |old_node: NodeIndex, search_map: &mut SearchGraph| -> NodeIndex {
            match route_old_to_new_nodes.get(&(key, old_node)) {
                Some(node) => *node,
                None => {
                let new_nodes = old_to_new_nodes.entry(old_node).or_insert(Vec::new());
                let route_node = search_map.add_node(SearchNode { route: key.clone(), old_node });
                new_nodes.push(route_node);
                route_old_to_new_nodes.insert((key, old_node), route_node);
                route_node
                }
            } 
        };
        
        for edge in route.station_to.values() {
            let (start, end) = subway_map.edge_endpoints(*edge).unwrap();
            let new_start_node = create_node(start, &mut search_map);
            let new_end_node = create_node(end, &mut search_map);
            let new_edge = search_map.add_edge(new_start_node, new_end_node, subway_map[*edge]);
            old_to_new_edges.entry(*edge).or_insert(Vec::new()).push(new_edge);
            new_to_old_edges.insert(new_edge, *edge);
        }
    }
    
    // Connect virtual nodes that correspond to the same station together with walk edges. This 
    // represents the transfer necessary to move between routes
    for related_nodes in old_to_new_nodes.values() {
        for i in 0..related_nodes.len()-1 {
            for j in i+1..related_nodes.len() {
                search_map.add_edge(related_nodes[i], related_nodes[j], Edge { ty: crate::EdgeType::Walk, weight: 1 });
            }
        }
    }
    
    // Create corresponding walk edges for those found on the original graph
    for edge in subway_map.edge_references() {
        if let EdgeType::Walk = edge.weight().ty {
            let node1 = edge.source();
            let node2 = edge.target();
            // ignore cases where no routes in the network use a station node
            if !(old_to_new_nodes.contains_key(&node1) && old_to_new_nodes.contains_key(&node2)) {
                continue;
            }
            let new_nodes1 = &old_to_new_nodes[&node1];
            let new_nodes2 = &old_to_new_nodes[&node2];
            // TODO: this causes quadratic blowup of walk edge numbers which is sometimes excessive.
            // Is this too aggressive? Can we just create one edge instead?
            for node1 in new_nodes1 {
                for node2 in new_nodes2 {
                    search_map.add_edge(*node1,*node2, *edge.weight());
                }
            }
        }
    }
    
    SearchMap { map: search_map, old_to_new_nodes, old_to_new_edges, new_to_old_edges }
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
// reconnect in the Bronx. For now the B service will always terminate earlier than this point. TODO
// lift this limitation?
pub fn shortest_paths(start: NodeIndex, end: NodeIndex, search_map: &mut SearchMap, mut k: usize) -> Vec<Vec<RoutePath>> {
    assert!(k >= 1);
    let valid_end_nodes: HashSet<_> = search_map.old_to_new_nodes[&end].clone().into_iter().collect();
    
    let start_nodes = &search_map.old_to_new_nodes[&start];

    // create virtual node + edges to represent start of search
    // note that route and old nodeindex are invalid for virtual node
    let virtual_start_node = search_map.map.add_node(SearchNode { route: String::new(), old_node: NodeIndex::new(0) });

    for start_node in start_nodes {
        search_map.map.add_edge(virtual_start_node, *start_node, Edge { ty: EdgeType::Walk, weight: 0 });
    }
    
    // TODO: should we consider route frequencies in this calculation? pass that data here if yes
    let (costs, destination) = dijkstra(&search_map.map, virtual_start_node, &valid_end_nodes, |edge| edge.weight().weight);
    let destination = match destination {
        Terminated::Exhaustive => panic!("did not encounter the desired destination in shortest path search"),
        Terminated::At(destination) => destination,
    };
    
    let routes = search_to_routes(search_map, &costs, destination);
    k -= 1;
    for route in &routes {
        if k == 0 {
            break;
        }

       // TODO calculate more routes by disabling edges 

        k -= 1;
    }

    search_map.map.remove_node(virtual_start_node);

    vec![routes]
}

#[derive(Debug)]
pub struct RoutePath {
    id: String,
    cost: u16,
    start_node: NodeIndex,
    end_node: NodeIndex,
    edge_to_next: Option<EdgeIndex>,
}

fn search_to_routes(search_map: &SearchMap, costs: &HashMap<NodeIndex, (u16, Option<EdgeIndex>)>, destination: NodeIndex) -> Vec<RoutePath> {
    let mut curr_edge = costs[&destination].1.unwrap();
    let mut routes = Vec::new();
    
    // These are updated as we're traversing a current route, and used to get the final route data
    let mut curr_end_node = search_map.map.edge_endpoints(curr_edge).unwrap().0; 
    let mut curr_start_node = curr_end_node;
    let mut curr_cost = 0;
    let mut curr_route = None;
    loop {
        let source_node = search_map.map.edge_endpoints(curr_edge).unwrap().0;
        let edge_ref = &search_map.map[curr_edge];
        if edge_ref.ty == EdgeType::Walk {
            if let Some(curr_route) = curr_route.take() {
                routes.push(RoutePath { id: curr_route, cost: curr_cost, start_node: curr_start_node, end_node: curr_end_node, edge_to_next: Some(curr_edge) });
            }
            curr_cost = 0;
        }
        else {
            if curr_route.is_none() {
                curr_route = Some(search_map.map[source_node].route.clone());
                curr_end_node = search_map.map.edge_endpoints(curr_edge).unwrap().1;
            }
            curr_start_node = search_map.map.edge_endpoints(curr_edge).unwrap().0;
            curr_cost += edge_ref.weight;
        }

        curr_edge = match costs[&source_node].1 {
            Some(curr_edge) => curr_edge,
            None => {
                // TODO is this necessary?
                // routes.push(RoutePath {id: curr_route.take().unwrap(), cost: curr_cost, start_node: curr_start_node, end_node: curr_end_node, edge_to_next: Some(curr_edge)});
                break;
            }
        };
    };

    routes.reverse();
    routes
}

const WALK_MULTIPLIER: f32 = 1.0;
const WAIT_MULTIPLIER: f32 = 1.0;

fn calculate_costs(search_map: &mut SearchMap, frequencies: &[HashMap<String, Cell<i64>>], routes: &HashMap<String, Route>, trip_data: &TripData) -> f32 {
    let mut total_cost = 0.;
    for (time, trips) in trip_data.iter() {
        for trip in trips {
            // TODO do more than one path?
            // TODO cache this information?
            let paths = shortest_paths(trip.start, trip.end, search_map, 1);
            assert!(!paths.is_empty());
            let mut lowest_cost = f32::INFINITY;
            for (i, path) in paths.iter().enumerate() {
                let mut curr_time = *time as f32;
                let mut cost = 0.;
                for segment in path {
                    let curr_schedule = curr_time as i64 / SCHEDULE_GRANULARITY;
                    let wait = SCHEDULE_GRANULARITY as f32 / frequencies[curr_schedule as usize][&segment.id].get() as f32 * WAIT_MULTIPLIER;
                    let total_segment_cost = segment.cost as f32 + wait;
                    cost += total_segment_cost;
                    curr_time += total_segment_cost;
                    if let Some(edge_idx) = segment.edge_to_next {
                        let walk_time = search_map.map[edge_idx].weight as f32 * WALK_MULTIPLIER;
                        cost += walk_time;
                        curr_time += walk_time;
                    }
                }
                lowest_cost = lowest_cost.min(cost);
            }
            total_cost += lowest_cost * trip.count as f32;
        }
    }
    total_cost
}
