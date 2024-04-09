// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod shortest_path;
mod simulator;

use petgraph::algo::{has_path_connecting, DfsSpace};
use simulator::{
    optimize, shortest_paths, Route, SimulationResults,
    Simulator, SubwayMap, TrackStationId, SCHEDULE_PERIOD,
};

use std::collections::{HashMap, HashSet};

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{Direction, Graph};
use serde::{Deserialize, Serialize};

use rand::rngs::StdRng;
use rand::seq::IteratorRandom;
use rand::{Rng, SeedableRng};

use crate::simulator::{SearchMap, Trip, TripData, SCHEDULE_GRANULARITY};

#[derive(Deserialize, Serialize, Clone, Hash, PartialEq, Eq)]
struct JsNode {
    id: String,
}

#[derive(Deserialize, Serialize, Clone, Hash, PartialEq, Eq)]
struct JsEdge {
    id: String,
    source: String,
    target: String,
    weight: u16,
    #[serde(alias = "type")]
    r#type: String,
}

impl JsEdge {
    fn to_edge(&self) -> Edge {
        Edge {
            ty: match &*self.r#type {
                "track" => EdgeType::Track,
                "walk" => EdgeType::Walk,
                _ => panic!("illegal walk type encountered"),
            },
            weight: self.weight,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum EdgeType {
    Track,
    Walk,
}

#[derive(Debug, Copy, Clone)]
pub struct Edge {
    ty: EdgeType,
    weight: u16,
}

#[derive(Deserialize, Serialize, Clone)]
struct JsRoute {
    name: String,
    id: String,
    nodes: Vec<String>,
    edges: Vec<String>,
    #[serde(default)]
    offset: u64,
}

#[derive(Deserialize, Serialize, Clone)]
struct JsGraph {
    nodes: Vec<JsNode>,
    edges: Vec<JsEdge>,
}

type JsRoutes = HashMap<String, JsRoute>;

#[derive(Serialize)]
struct ShortestPath {
    length: u16,
    path: Vec<String>,
}

// TODO: clean up HashMap return situation
fn js_graph_to_subway_map(
    js_graph: JsGraph,
) -> (
    SubwayMap,
    HashMap<String, NodeIndex>,
    HashMap<TrackStationId, String>,
) {
    let mut graph: SubwayMap = Graph::new();
    let mut cytoscape_map = HashMap::new();
    let mut petgraph_map = HashMap::new();
    for node in js_graph.nodes {
        let node_id = graph.add_node(node.id.clone());
        cytoscape_map.insert(node.id.clone(), node_id);
        petgraph_map.insert(TrackStationId::Station(node_id), node.id);
    }
    for edge in js_graph.edges {
        let edge_id = graph.add_edge(
            *cytoscape_map.get(&edge.source).unwrap(),
            *cytoscape_map.get(&edge.target).unwrap(),
            edge.to_edge(),
        );
        petgraph_map.insert(TrackStationId::Track(edge_id), edge.id.clone());

        // walk edges are represented as one-way in JsGraph for creation convenience; they must
        // be duplicated in the other direction to represent their two-way nature
        if edge.r#type == "walk" {
            let edge_id = graph.add_edge(
                *cytoscape_map.get(&edge.target).unwrap(),
                *cytoscape_map.get(&edge.source).unwrap(),
                edge.to_edge(),
            );
            petgraph_map.insert(TrackStationId::Track(edge_id), edge.id.clone() + "_rev");
        }
    }
    (graph, cytoscape_map, petgraph_map)
}

#[tauri::command]
fn shortest_path(js_graph: JsGraph, js_routes: JsRoutes, source: String, target: String) {
    let (graph, map, _) = js_graph_to_subway_map(js_graph);
    let (routes, _) = js_routes_to_routes(js_routes, &graph, &map);
    let mut search_map = SearchMap::generate(&graph, &routes);
    let start = map[&source];
    let end = map[&target];
    let paths = shortest_paths(start, end, &mut search_map, 3);
    println!("Shortest paths: {:?}", paths);
}

fn js_routes_to_routes(
    js_routes: JsRoutes,
    subway_map: &SubwayMap,
    cytoscape_id_map: &HashMap<String, NodeIndex>,
) -> (Vec<Route>, Vec<String>) {
    let mut route_id_map = Vec::new();
    let mut routes = Vec::new();
    for (_, route) in js_routes {
        let mut station_to = HashMap::with_capacity(route.nodes.len());
        let node_ids: HashSet<_> = route.nodes.iter().map(|id| cytoscape_id_map[id]).collect();

        for node in &node_ids {
            for neighbor_edge in subway_map.edges_directed(*node, Direction::Outgoing) {
                if node_ids.contains(&neighbor_edge.target()) {
                    station_to.insert(*node, neighbor_edge.id());
                    break;
                }
            }
        }

        routes.push(Route {
            name: route.name,
            start_station: cytoscape_id_map[&route.nodes[0]],
            station_to,
            offset: route.offset,
        });
        route_id_map.push(route.id.clone());
    }
    (routes, route_id_map)
}

fn simulation_results_to_js(
    simulation_results: SimulationResults,
    petgraph_map: &HashMap<TrackStationId, String>,
    route_id_map: &[String],
) -> JsSimulationResults {
    let train_positions: Vec<_> = simulation_results
        .train_positions
        .into_iter()
        .map(|t| JsTrainPositions {
            time: t.time,
            trains: t
                .trains
                .into_iter()
                .map(|p| JsTrainPosition {
                    id: (p.id.route_idx, p.id.count),
                    curr_section: petgraph_map[&p.curr_section].clone(),
                    pos: p.pos,
                    distance_travelled: p.distance_travelled,
                })
                .collect(),
        })
        .collect();

    let train_to_route = simulation_results
        .train_to_route
        .into_iter()
        .map(|(train, route)| {
            (
                format!("{}_{}", train.route_idx, train.count),
                route_id_map[route.0 as usize].clone(),
            )
        })
        .collect();

    let station_statistics = simulation_results
        .station_statistics
        .into_iter()
        .map(|(id, s)| {
            let overall_arrival_times = (s.arrival_times.len() > 1).then(|| {
                let mut data = Vec::new();
                s.arrival_times
                    .values()
                    .for_each(|arrival_time| data.extend(arrival_time));
                data.sort_unstable_by(f64::total_cmp);
                calculate_arrival_time_statistics(data)
            });
            let arrival_times = s
                .arrival_times
                .into_iter()
                .map(|(r_id, data)| {
                    (
                        route_id_map[r_id.0 as usize].clone(),
                        calculate_arrival_time_statistics(data),
                    )
                })
                .collect();
            (
                petgraph_map[&TrackStationId::Station(id)].clone(),
                JsStationStatistic {
                    arrival_times,
                    overall_arrival_times,
                },
            )
        })
        .collect();

    JsSimulationResults {
        train_positions,
        train_to_route,
        station_statistics,
    }
}

#[tauri::command]
async fn run_simulation(
    js_graph: JsGraph,
    js_routes: JsRoutes,
    frequency: u64,
) -> Result<JsSimulationResults, String> {
    let (subway_map, cytoscape_id_map, petgraph_map) = js_graph_to_subway_map(js_graph.clone());
    let (routes, route_id_map) = js_routes_to_routes(js_routes, &subway_map, &cytoscape_id_map);

    let simulator = Simulator::new(subway_map, routes.clone());
    let simulation_results = simulator.run(60, frequency);
    Ok(simulation_results_to_js(
        simulation_results,
        &petgraph_map,
        &route_id_map,
    ))
}

#[tauri::command]
async fn run_optimize(
    js_graph: JsGraph,
    js_routes: JsRoutes,
) -> Result<JsSimulationResults, String> {
    let (subway_map, cytoscape_id_map, petgraph_map) = js_graph_to_subway_map(js_graph.clone());
    let (routes, route_id_map) = js_routes_to_routes(js_routes, &subway_map, &cytoscape_id_map);

    let mut rng = StdRng::seed_from_u64(5050);

    let mut trip_data = TripData::new();
    let mut num_trips = 0;
    
    let mut search_map = SearchMap::generate(&subway_map, &routes);

    for _ in 0..30 * SCHEDULE_PERIOD {
        let start = subway_map.node_indices().choose(&mut rng).unwrap();
        let end = subway_map.node_indices().choose(&mut rng).unwrap();
        
        if !shortest_paths(start, end, &mut search_map, 1).is_empty() {
            let trip = Trip { start,
                end,
                count: 1,
            };
            trip_data
                .entry(rng.gen_range(SCHEDULE_GRANULARITY*2..SCHEDULE_PERIOD))
                .or_default()
                .push(trip);
            num_trips += 1;
        }
    }
    
    println!("Using {num_trips} trips for optimization");

    let (schedule, simulation_results) = optimize(subway_map, routes, &trip_data);

    println!("Found schedule: {:#?}", schedule);

    // TODO handle error condition
    let simulation_results = simulation_results.unwrap();
    Ok(simulation_results_to_js(
        simulation_results,
        &petgraph_map,
        &route_id_map,
    ))
}

#[derive(Serialize)]
struct JsTrainPosition {
    pub id: (u32, u32),
    pub curr_section: String,
    pub pos: f64,
    pub distance_travelled: f64,
}

#[derive(Serialize)]
struct JsTrainPositions {
    pub time: u32,
    pub trains: Vec<JsTrainPosition>,
}

#[derive(Serialize)]
struct JsStationStatistic {
    pub arrival_times: HashMap<String, JsArrivalStats>,
    /// arrival times for all routes
    /// None if there's only one route
    pub overall_arrival_times: Option<JsArrivalStats>,
}

#[derive(Serialize)]
struct JsArrivalStats {
    pub min_wait: f64,
    pub max_wait: f64,
    pub average_wait: f64,
}

#[derive(Serialize)]
struct JsSimulationResults {
    pub train_positions: Vec<JsTrainPositions>,
    // String of routeid_trainnum to route string
    pub train_to_route: HashMap<String, String>,
    pub station_statistics: HashMap<String, JsStationStatistic>,
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            run_simulation,
            shortest_path,
            run_optimize
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn calculate_arrival_time_statistics(data: Vec<f64>) -> JsArrivalStats {
    let mut differences = Vec::with_capacity(data.len());
    let mut prev_time = data.first().copied().unwrap_or_default();
    for item in data.iter().skip(1) {
        differences.push(*item - prev_time);
        prev_time = *item;
    }
    JsArrivalStats {
        min_wait: differences
            .iter()
            .copied()
            .min_by(f64::total_cmp)
            .unwrap_or_default(),
        max_wait: differences
            .iter()
            .copied()
            .max_by(f64::total_cmp)
            .unwrap_or_default(),
        average_wait: differences.iter().sum::<f64>() / differences.len() as f64,
    }
}
