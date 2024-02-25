// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod simulator;
use simulator::{Route, Simulator, SubwayMap, TrackStationId};

use std::collections::{HashMap, HashSet};

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{Direction, Graph};
use serde::{Deserialize, Serialize};

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
enum EdgeType {
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
    }
    (graph, cytoscape_map, petgraph_map)
}

#[allow(unused)]
fn shortest_path() -> Option<ShortestPath> {
    /* 
    let (graph, map, _) = js_graph_to_subway_map(js_graph);

    let end = *map.get(target).unwrap();

    let (length, path) = astar(
        &graph,
        *map.get(source).unwrap(),
        |id| id == end,
        |edge| *edge.weight(),
        |_| 0,
    )?;

    let mut result = Vec::with_capacity(path.len());
    for node in path {
        result.push(graph[node].clone());
    }

    Some(ShortestPath {
        length,
        path: result,
    })
    */
    None
}

#[tauri::command]
async fn run_simulation(
    js_graph: JsGraph,
    js_routes: JsRoutes,
    frequency: u64
) -> Result<JsSimulationResults, String> {
    eprintln!("start running simulation");
    let mut routes = Vec::with_capacity(js_routes.len());
    let (subway_map, cytoscape_id_map, petgraph_map) = js_graph_to_subway_map(js_graph.clone());
    
    let mut route_id_map = Vec::new();

    for (_, route) in js_routes {
        let mut station_to = HashMap::with_capacity(route.nodes.len());
        let node_ids: HashSet<_> = route
            .nodes
            .iter()
            .map(|id| cytoscape_id_map[id])
            .collect();

        for node in &node_ids {
            for neighbor_edge in subway_map.edges_directed(*node, Direction::Outgoing) {
                if node_ids.contains(&neighbor_edge.target()) {
                    station_to.insert(*node, neighbor_edge.id());
                    break;
                }
            }
        }

        // TODO: is this restriction overly limiting?
        routes.push(Route {
            start_station: cytoscape_id_map[&route.nodes[0]],
            station_to,
            offset: route.offset,
        });
        route_id_map.push(route.id.clone());
    }
    let simulator = Simulator::new(subway_map, routes.clone());
    let simulation_results = simulator.run(60, frequency);
    let train_positions: Vec<_> = simulation_results
        .train_positions
        .into_iter()
        .map(|t| JsTrainPositions {
            time: t.time,
            trains: t
                .trains
                .into_iter()
                .map(|p| JsTrainPosition {
                    id: p.id.0,
                    curr_section: petgraph_map[&p.curr_section].clone(),
                    pos: p.pos,
                    distance_travelled: p.distance_travelled,
                })
                .collect(),
        })
        .collect();
    
    let train_to_route = simulation_results.train_to_route.into_iter().map(|(train, route)| (train.0, route_id_map[route.0 as usize].clone())).collect();

    let station_statistics = simulation_results
        .station_statistics
        .into_iter()
        .map(|(id, s)| {
            
            let overall_arrival_times = (s.arrival_times.len() > 1).then(|| {
                let mut data = Vec::new();
                s.arrival_times.values().for_each(|arrival_time| data.extend(arrival_time));
                data.sort_unstable_by(f64::total_cmp);
                calculate_arrival_time_statistics(data)
            });
            let arrival_times = s.arrival_times.into_iter().map(|(r_id, data)| {

                        (
                            route_id_map[r_id.0 as usize].clone(),
                            calculate_arrival_time_statistics(data)
                        )
                    }).collect();
            (
                petgraph_map[&TrackStationId::Station(id)].clone(),
                JsStationStatistic {
                    arrival_times,
                    overall_arrival_times
                },
            )
        })
        .collect();

    Ok(JsSimulationResults {
        train_positions,
        train_to_route,
        station_statistics,
    })
}

#[derive(Serialize)]
struct JsTrainPosition {
    pub id: u32,
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
    pub train_to_route: HashMap<u32, String>,
    pub station_statistics: HashMap<String, JsStationStatistic>,
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_simulation])
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
        min_wait: differences.iter().copied().min_by(f64::total_cmp).unwrap_or_default(),
        max_wait: differences.iter().copied().max_by(f64::total_cmp).unwrap_or_default(),
        average_wait: differences.iter().sum::<f64>() / differences.len() as f64,
    }
}
