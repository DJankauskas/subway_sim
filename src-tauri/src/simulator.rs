use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use petgraph::Graph;

pub type SubwayMap = Graph<String, u16>;
pub type StationId = NodeIndex<u32>;
pub type TrackId = EdgeIndex;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TrainId(pub u32);
#[derive(Debug)]
pub struct Train {
    pub id: TrainId,
    /// The current node or track the train is on
    pub curr_section: TrackStationId,
    /// The position of the train at its current section
    pub pos: f64,
    /// The current route the train is on
    pub route: RouteId,
}

#[derive(Debug)]
pub struct Track {
    pub id: TrackId,
    pub length: u16,
    pub trains: VecDeque<TrainId>,
}

#[derive(Debug)]
pub struct Station {
    pub id: StationId,
    pub train: Option<TrainId>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TrackStationId {
    Track(TrackId),
    Station(StationId),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RouteId(u32);

#[derive(Debug)]
pub struct Route {
    pub start_station: StationId,
    pub station_to: HashMap<StationId, TrackId>,
}

#[derive(Debug)]
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
}

const STATION_DWELL_TIME: f64 = 1.0;
const MIN_TRAIN_DISTANCE: f64 = 1.0;
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
                },
            );
        }

        for edge in subway_map.edge_references() {
            tracks.insert(
                edge.id(),
                Track {
                    id: edge.id(),
                    length: *edge.weight(),
                    trains: VecDeque::new(),
                },
            );
        }

        let terminal_nodes = terminal_nodes(&subway_map);
        let mut queue: VecDeque<TrackStationId> = terminal_nodes
            .into_iter()
            .map(|n| TrackStationId::Station(n))
            .collect();
        let mut traversal_order: Vec<TrackStationId> = Vec::new();
        let mut visited = HashSet::new();

        while let Some(track_station) = queue.pop_front() {
            if visited.contains(&track_station) {
                continue;
            };
            visited.insert(track_station);
            traversal_order.push(track_station.clone());
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

    pub fn run(&mut self, iterations: i32) -> Vec<TrainPositions> {
        let traversal_order = self.traversal_order.clone();
        println!("{:?}", traversal_order);
        for (_, route) in &self.routes {
            println!("{:?}", route.start_station);
        }

        let mut train_positions = Vec::new();

        let mut t = -60;

        while t < iterations {
            for track_station in &traversal_order {
                match *track_station {
                    TrackStationId::Station(station) => {
                        let mut time_left = TIME_STEP;
                        if let Some(train) = &self.stations[&station].train {
                            let train = *train;
                            let train_mut = self.trains.get_mut(&train).unwrap();
                            let distance_travelled =
                                f64_max(STATION_DWELL_TIME - train_mut.pos, 0.0);
                            train_mut.pos += distance_travelled;
                            time_left -= distance_travelled;
                            let route_id = train_mut.route;
                            let next_track_id = self.routes[&route_id].station_to.get(&station);
                            let next_track_id = match next_track_id {
                                Some(next_track_id) => next_track_id,
                                None => {
                                    self.trains.remove(&train);
                                    self.stations.get_mut(&station).unwrap().train = None;
                                    continue;
                                }
                            };
                            let next_track: &mut Track =
                                self.tracks.get_mut(&next_track_id).unwrap();
                            let last_train = next_track.trains.back();
                            if let Some(last_train) = last_train {
                                let last_train_pos = self.trains[last_train].pos;
                                // only move the train off the station if there's space on the next track
                                if last_train_pos >= MIN_TRAIN_DISTANCE {
                                    self.stations.get_mut(&station).unwrap().train = None;
                                    next_track.trains.push_back(train);
                                    let pos_move =
                                        f64_min(time_left, last_train_pos - MIN_TRAIN_DISTANCE);
                                    let train_mut = self.trains.get_mut(&train).unwrap();
                                    train_mut.pos = pos_move;
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
                    TrackStationId::Track(track) => {
                        let track_mut = self.tracks.get_mut(&track).unwrap();
                        let mut i = 0;
                        let mut last_train_pos = f64::INFINITY;
                        let next_station_id = self.subway_map.edge_endpoints(track).unwrap().1;
                        if self.stations[&next_station_id].train.is_some() {
                            last_train_pos = track_mut.length as f64;
                        }
                        while i < track_mut.trains.len() {
                            let curr_train_id = track_mut.trains[i];
                            let curr_train_mut = self.trains.get_mut(&curr_train_id).unwrap();
                            let mut time_left = TIME_STEP;
                            let travel_distance = f64_min(
                                time_left,
                                f64_max(
                                    last_train_pos - curr_train_mut.pos - MIN_TRAIN_DISTANCE,
                                    0.0,
                                ),
                            );
                            curr_train_mut.pos += travel_distance;
                            time_left -= travel_distance;
                            // we're done with the current track, and need to move into the station
                            if curr_train_mut.pos >= track_mut.length as f64 {
                                debug_assert_eq!(i, 0);
                                track_mut.trains.pop_front();
                                debug_assert!(self
                                    .stations
                                    .get_mut(&next_station_id)
                                    .unwrap()
                                    .train
                                    .is_none());
                                self.stations.get_mut(&next_station_id).unwrap().train =
                                    Some(curr_train_id);
                                curr_train_mut.pos = time_left;
                                curr_train_mut.curr_section =
                                    TrackStationId::Station(next_station_id);
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
            if t % 3 == 0 {
                for (id, route) in &self.routes {
                    let start_station_mut = self.stations.get_mut(&route.start_station).unwrap();
                    // TODO: do I need to handle the case where this is not true?
                    if start_station_mut.train.is_none() {
                        let train = Train {
                            id: self.curr_train_id,
                            curr_section: TrackStationId::Station(start_station_mut.id),
                            pos: 0.0,
                            route: *id,
                        };

                        start_station_mut.train = Some(self.curr_train_id);
                        self.trains.insert(self.curr_train_id, train);
                        self.curr_train_id = TrainId(self.curr_train_id.0 + 1);
                    }
                }
            }

            println!("Iteration: {t}, train count: {}", self.trains.len());

            let mut curr_train_positions = Vec::new();
            for (id, train) in &self.trains {
                curr_train_positions.push(TrainPosition {
                    id: *id,
                    curr_section: train.curr_section,
                    pos: train.pos,
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
        train_positions
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
