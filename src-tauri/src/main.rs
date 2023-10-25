// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod simulator;
use simulator::{Route, Simulator, SubwayMap};

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
    routes: HashMap<String, JsRoute>,
}

#[derive(Serialize)]
struct ShortestPath {
    length: u16,
    path: Vec<String>,
}

// TODO: clean up HashMap return situation
fn js_graph_to_subway_map(js_graph: JsGraph) -> (SubwayMap, HashMap<String, NodeIndex>) {
    let mut graph: SubwayMap = Graph::new();
    let mut map = HashMap::new();
    for node in js_graph.nodes {
        let node_id = graph.add_node(node.id.clone());
        map.insert(node.id, node_id);
    }
    for edge in js_graph.edges {
        graph.add_edge(
            *map.get(&edge.source).unwrap(),
            *map.get(&edge.target).unwrap(),
            edge.weight,
        );
    }
    (graph, map)
}

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn shortest_path(js_graph: JsGraph, source: &str, target: &str) -> Option<ShortestPath> {
    let (graph, map) = js_graph_to_subway_map(js_graph);

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
async fn run_simulation(js_graph: JsGraph) -> Result<(), String> {
    eprintln!("start running simulation");
    let mut routes = Vec::with_capacity(js_graph.routes.len());
    let (subway_map, node_id_map) = js_graph_to_subway_map(js_graph.clone());

    for (_, route) in js_graph.routes {
        let mut station_to = HashMap::with_capacity(route.nodes.len());
        let node_ids: HashSet<_> = route.nodes.into_iter().map(|id| node_id_map[&id]).collect();

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
    simulator.run(360);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![shortest_path, run_simulation])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
