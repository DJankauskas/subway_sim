// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod simulator;
use simulator::{Route, Simulator, SubwayMap, TrackStationId};

use std::collections::{HashMap, HashSet};

use petgraph::algo::astar;
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
}

#[derive(Deserialize, Serialize, Clone)]
struct JsRoute {
    name: String,
    id: String,
    nodes: Vec<String>,
    edges: Vec<String>,
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
            edge.weight,
        );
        petgraph_map.insert(TrackStationId::Track(edge_id), edge.id.clone());
    }
    (graph, cytoscape_map, petgraph_map)
}

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn shortest_path(js_graph: JsGraph, source: &str, target: &str) -> Option<ShortestPath> {
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
}

#[tauri::command]
async fn run_simulation(js_graph: JsGraph, js_routes: JsRoutes) -> Result<Vec<JsTrainPositions>, String> {
    eprintln!("start running simulation");
    let mut routes = Vec::with_capacity(js_routes.len());
    let (subway_map, cytoscape_id_map, petgraph_map) = js_graph_to_subway_map(js_graph.clone());

    for (_, route) in js_routes {
        let mut station_to = HashMap::with_capacity(route.nodes.len());
        let node_ids: HashSet<_> = route
            .nodes
            .into_iter()
            .map(|id| cytoscape_id_map[&id])
            .collect();

        let mut start_station = None;

        for node in &node_ids {
            if subway_map
                .edges_directed(*node, Direction::Incoming)
                .next()
                .is_none()
            {
                start_station = Some(*node);
            }
            for neighbor_edge in subway_map.edges_directed(*node, Direction::Outgoing) {
                if node_ids.contains(&neighbor_edge.target()) {
                    station_to.insert(*node, neighbor_edge.id());
                    break;
                }
            }
        }

        // TODO: is this restriction overly limiting?
        routes.push(Route {
            start_station: start_station.expect("a station in the route with no incoming edges"),
            station_to,
        })
    }
    let mut simulator = Simulator::new(subway_map, routes);
    let train_positions = simulator.run(90);
    let js_train_positions: Vec<_> = train_positions
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
                })
                .collect(),
        })
        .collect();

    Ok(js_train_positions)
}

#[derive(Serialize)]
struct JsTrainPosition {
    pub id: u32,
    pub curr_section: String,
    pub pos: f64,
}

#[derive(Serialize)]
struct JsTrainPositions {
    pub time: u32,
    pub trains: Vec<JsTrainPosition>,
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![shortest_path, run_simulation])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
