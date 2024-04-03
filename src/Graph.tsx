import { useRef, MutableRefObject, useEffect, useState } from "react";
import cytoscape, { Core, EdgeSingular, NodeSingular } from "cytoscape";
import popper from "cytoscape-popper";
cytoscape.use(popper);
import tippy from "tippy.js";
import { SubwayGraph, Route, Routes } from "./subwayGraph";
import { createRoot } from "react-dom/client";

import { StringlineChart, Stringline, StringlinePoint, Station } from "./StringlineChart";

export type GraphMode = 'edit' | 'path_select' | 'route_edit' | 'display'

export interface TrainPositions {
    time: number,
    trains: TrainPosition[]
}

export interface TrainPosition {
    id: [number, number],
    curr_section: string,
    pos: number,
    distance_travelled: number,
}

export interface SimulationResults {
    train_positions: TrainPositions[],
    train_to_route: Record<string, string>,
    station_statistics: Record<string, StationStatistic>,
}

interface ArrivalTimes {
    min_wait: number,
    max_wait: number,
    average_wait: number
}

interface StationStatistic {
    arrival_times: Record<string, ArrivalTimes>,
    overall_arrival_times?: ArrivalTimes
}

type GraphProps = {
    initialSubwayGraph: SubwayGraph,
    mode: GraphMode,
    onShortestPath: (graph: any, routes: any, source: string, target: string) => void,
    onSimulate: (graph: any, routes: any, frequency: number) => Promise<SimulationResults>,
    onOptimize: (graph: any, routes: any) => Promise<SimulationResults>,
    getCurrentSubwayGraph?: MutableRefObject<() => SubwayGraph>,
}

// escape -> clear selection, clear state 
// n -> new node 
// select a, e, select b -> edge between a and b
// select node or edge, delete -> delete the selected item 


type EditType = { type: 'edgeCreate', edgeSourceNode: NodeSingular } | { type: 'edgeWeight', edge: EdgeSingular, renderedX: number, renderedY: number, weight: string } | { type: 'nodeName', node: NodeSingular, renderedX: number, renderedY: number, name: string } | { type: 'none' };

export const Graph = ({ initialSubwayGraph, mode, onSimulate, onOptimize, onShortestPath, getCurrentSubwayGraph }: GraphProps) => {
    const divRef = useRef(null);
    const [graph, setGraph] = useState<Core | undefined>();
    const [editType, setEditType] = useState<EditType>({ type: 'none' });
    const [routes, setRoutes] = useState<Routes>({});
    const [routeOffsets, setRouteOffsets] = useState<Record<string, number>>({});
    const [frequency, setFrequency] = useState("4");
    const [simulationResults, setSimulationResults] = useState<SimulationResults | null>(null);
    const [primaryStringlineRoute, setPrimaryStringlineRoute] = useState<string | undefined>(undefined);
    const [secondaryStringlineRoute, setSecondaryStringlineRoute] = useState<string | undefined>(undefined);
    const [routeToEdit, setRouteToEdit] = useState("");
    const [newRouteName, setNewRouteName] = useState("")
    const [routeColor, setRouteColor] = useState("red");


    // Update upstream graph with current graph state
    useEffect(() => {
        if (graph && getCurrentSubwayGraph) {
            getCurrentSubwayGraph.current = () => graphToSubwayGraph(graph, routes);
        }
    }, [graph, getCurrentSubwayGraph, routes]);

    // Handle keyboard events
    useEffect(() => {
        const keydownHandler = (event: KeyboardEvent) => {
            if (graph && mode === 'edit') {
                const selected =
                    graph.$(':selected')[0];
                switch (event.key) {
                    case 'Escape':
                        setEditType({ type: 'none' });
                        break;
                    case 'Backspace':
                        selected.remove()
                        break;
                    case 'n':
                        graph.add({ data: { name: '' }, position: currentCenter(graph) })
                        break;
                    case 'w':
                        if (selected.isEdge()) {
                            selected.data("type", "walk");
                        }
                        break;
                    case 't':
                        if (selected.isEdge()) {
                            selected.data("type", "track");
                        }
                        break;
                    case 'e':
                        if (selected.isNode()) {
                            setEditType({ type: 'edgeCreate', edgeSourceNode: selected })
                        }
                        break;
                    default:
                }
            } else if (mode === 'route_edit') {
                if (event.key === 'Enter') {
                    const selected = graph!.$(':selected');
                    const route = routes[routeToEdit];

                    const nodes = new Set<string>();
                    const edges = new Set<string>();

                    selected.filter('node').forEach(node => { nodes.add(node.id()) });
                    selected.filter('edge').forEach(edge => { edges.add(edge.id()) });
                    
                    // find the node that is not pointed to by any edge; it should be the first
                    const destinationNodes = new Set();
                    for (const edge of edges) {
                        destinationNodes.add(graph?.$id(edge).target().id());
                    }
                    
                    const nodesDifference = new Set<string>();

                    for (const node of nodes) {
                        if (!destinationNodes.has(node)) {
                            nodesDifference.add(node);
                        }
                    }
                    
                    if (nodesDifference.size != 1) {
                        // TODO better message and abort route edit
                        alert("Creating a route with dangling nodes or that is cyclic");
                    }
                    
                    const nodesSorted = [];
                    const edgesSorted = [];
                    
                    let currNode = Array.from(nodesDifference)[0] as string | undefined;
                    while (currNode) {
                        nodesSorted.push(currNode);
                        const edge = graph?.$id(currNode).connectedEdges(`edge[type = "track"][source = "${currNode}"]`).filter(edge => edges.has(edge.id()))[0];
                        if (edge) {
                            edgesSorted.push(edge.id());
                            currNode = edge.target().id();
                        } else {
                            currNode = undefined;
                        }
                    }

                    if (nodesSorted.length != nodes.size || edgesSorted.length != edges.size) {
                        // TODO better message and abort route edit
                        alert(`oops: bug in the sorting code ${nodesSorted.length} ${nodes.size} ${edgesSorted.length} ${edges.size}`);
                    }

                    selected.unselect();
                    
                    console.log("Setting color to ", routeColor);

                    setRoutes({ ...routes, [route.id]: {...route, nodes: nodesSorted, edges: edgesSorted, color: routeColor} });
                }
            }
            if (event.key === 'r') {
                // reset viewport settings
                graph?.reset();
            }
        };
        document.addEventListener('keydown', keydownHandler);
        return () => {
            document.removeEventListener('keydown', keydownHandler);
        };
    }, [graph, mode, editType, routeColor])

    // Set up graph styles on component initialization
    useEffect(() => {
        if (divRef.current) {
            const graph = cytoscape({
                container: divRef.current,
                elements: [],
                style: [
                    {
                        selector: 'edge',
                        style: {
                            'label': 'data(weight)',
                            'text-rotation': 'autorotate',
                            'target-arrow-shape': 'triangle',
                            'curve-style': 'bezier'
                        },
                    },
                    {
                        selector: 'edge[type="walk"]',
                        style: {
                            'line-style': 'dashed',
                            'target-arrow-shape': 'none',
                        },
                    },
                    {
                        selector: 'node',
                        style: {
                            'label': 'data(name)',
                            'text-valign': 'center',
                            'text-halign': 'center',
                        }
                    },
                    {
                        selector: 'node[type="train"]',
                        style: {
                            'background-color': 'data(color)'
                        }
                    }
                ]
            });
            setGraph(graph);
        }
    }, [divRef.current])

    // On graph change, re-initialize the cytoscape graph with new data
    useEffect(() => {
        if (graph && initialSubwayGraph) {
            initializeGraph(graph, initialSubwayGraph);
            setRoutes(initialSubwayGraph.routes);
            graph.reset();
        }
        return () => { graph?.elements().remove(); }
    }, [graph, initialSubwayGraph])

    // Handle node and edge clicks and double clicks
    useEffect(() => {
        if (graph) {
            const selectHandler = () => {
                if (mode === 'edit') {
                    if (editType.type === 'edgeCreate') {
                        const target = graph.$(':selected')[0]
                        if (target && target.isNode()) {
                            graph.add({ data: { source: editType.edgeSourceNode.id(), target: target.id(), weight: parseInt(prompt("Edge weight") || "3"), type: "track" } });
                        }
                    }
                } else if (mode === 'path_select') {
                    const selected = graph.$(':selected');
                    if (selected.length === 2) {
                        const g =
                            serializeGraph(graph);
                        onShortestPath(
                            g,
                            routes,
                            selected[0].id(),
                            selected[1].id()
                        );
                        selected.unselect();
                    }
                }
                setEditType({ type: 'none' });
            };
            const edgeDblclickHandler = (event: any) => {
                const currentEdge = event.target;
                const { x, y } = event.renderedPosition;
                setEditType({ type: 'edgeWeight', edge: currentEdge, renderedX: x, renderedY: y, weight: currentEdge.data("weight") })
            };
            const nodeClickHandler = (event: any) => {
                const currentNode = event.target;
                const statistic = simulationResults?.station_statistics[currentNode.id()];
                if (statistic) {
                    const ref = currentNode.popperRef();
                    const dummyEle = document.createElement('div');
                    const content = document.createElement('div');
                    const tooltipRoot = createRoot(content);
                    content.style.backgroundColor = 'white';
                    content.style.borderRadius = '5px';
                    content.style.padding = '2.5px';
                    content.style.boxShadow = '0 4px 8px rgba(0, 0, 0, 0.15), 0 1px 3px rgba(0, 0, 0, 0.1)';
                    const tip = tippy(dummyEle, {
                        getReferenceClientRect: ref.getBoundingClientRect,
                        trigger: 'manual',
                        content: () => {
                            tooltipRoot.render(<StationStatisticTooltip statistic={statistic} routes={routes} />)
                            return content;
                        },
                        onDestroy: () => {
                            tooltipRoot.unmount();
                        }
                    });
                    tip.show();
                    console.log(routes);
                    console.log(statistic);
                }
            }
            const nodeDblclickHandler = (event: any) => {
                const currentNode = event.target;
                const { x, y } = event.renderedPosition;
                setEditType({ type: 'nodeName', node: currentNode, renderedX: x, renderedY: y, name: currentNode.data("name") })
            }
            graph.on('select', selectHandler);
            graph.on('dblclick', 'edge', edgeDblclickHandler);
            graph.on('click', 'node', nodeClickHandler);
            graph.on('dblclick', 'node', nodeDblclickHandler);
            return () => {
                graph.removeListener('select', selectHandler)
                    .removeListener('dblclick', edgeDblclickHandler)
                    .removeListener('click', nodeClickHandler)
                    .removeListener('dblclick', nodeDblclickHandler);
            }
        }
    }, [mode, routes, editType, simulationResults, onShortestPath])

    // Update select type on mode change
    useEffect(() => {
        if (graph) {
            if (mode === 'path_select' || mode === 'route_edit') {
                graph.selectionType('additive')
            } else {
                graph.selectionType('single')
            }
        }
    }, [mode]);

    return (
        <div>
            <div ref={divRef} style={{ width: 800, height: 400 }}></div>
            <div>
                <button style={{marginRight: 10}} onClick={async () => {
                    if (graph) {
                        const routesWithOffsets = {} as Record<string, Route & { offset: number }>;
                        for (const [id, route] of Object.entries(routes)) {
                            routesWithOffsets[id] = { ...route, offset: routeOffsets[id] || 0 };
                        }
                        const results = await onSimulate(serializeGraph(graph), routesWithOffsets, parseInt(frequency) || 4);
                        renderTrainPositions(graph, results, routes);
                        setSimulationResults(results);
                    }
                }}>Simulate</button>
                <label>
                    Simulation frequency:
                    <input style={{marginLeft: 5}} type="number" pattern="[0-9]*" placeholder="Frequency" value={frequency} onChange={e => setFrequency(e.currentTarget.value)} />
                </label>
            </div>
            <button style={{marginTop: 5}} onClick={async () => {
                if (graph) {
                    const routesWithOffsets = {} as Record<string, Route & { offset: number }>;
                    for (const [id, route] of Object.entries(routes)) {
                        routesWithOffsets[id] = { ...route, offset: routeOffsets[id] || 0 };
                    }
                    const results = await onOptimize(serializeGraph(graph), routesWithOffsets);
                    renderTrainPositions(graph, results, routes);
                    setSimulationResults(results);
                }
            }}>Optimize</button>
            <div hidden={true}>
                {Object.entries(routes).map(([key, data]) => <div key={key}><div>{data.name}</div><input placeholder="Offset" type="number" pattern="[0-9]*" value={routeOffsets[key] || ""} onChange={e => setRouteOffsets({ ...routeOffsets, [key]: parseInt(e.currentTarget.value) })} /></div>)}
            </div>
            {simulationResults ?
                (
                    <>
                        <RouteSelector route={primaryStringlineRoute} setRoute={setPrimaryStringlineRoute} routes={routes} />
                        <RouteSelector route={secondaryStringlineRoute} setRoute={setSecondaryStringlineRoute} routes={routes} />
                        <StringlineChart
                            stations={primaryStringlineRoute ? namedStationsOfRoute(routes[primaryStringlineRoute], initialSubwayGraph) : []}
                            stringlines={trainPositionsToStringlines(simulationResults.train_positions, simulationResults.train_to_route, 60, new Set(primaryStringlineRoute ? [primaryStringlineRoute, ...(secondaryStringlineRoute ? [secondaryStringlineRoute] : [])] : []), routes)}
                        />
                    </>
                )
                : null}
            {editType.type === 'edgeWeight'
                ? <GraphPropertyInput
                    type="number"
                    onChange={value => setEditType({ ...editType, weight: value })}
                    onSubmit={() => {
                        const parsedWeight = parseInt(editType.weight);
                        if (parsedWeight) {
                            editType.edge.data('weight', editType.weight);
                        }
                        setEditType({ type: 'none' })
                    }}
                    onCancel={() => setEditType({ type: 'none' })}
                    value={editType.weight}
                    x={editType.renderedX}
                    y={editType.renderedY}
                />
                : null}
            {editType.type === 'nodeName'
                ? <GraphPropertyInput
                    type="text"
                    onChange={value => setEditType({ ...editType, name: value })}
                    onSubmit={() => {
                        editType.node.data('name', editType.name);
                        setEditType({ type: 'none' })
                    }}
                    onCancel={() => setEditType({ type: 'none' })}
                    value={editType.name}
                    x={editType.renderedX}
                    y={editType.renderedY}
                />
                : null}
            {mode == 'route_edit' ? <>
                <div>
                    <RouteSelector routes={routes} route={routeToEdit} label="Route:" setRoute={(route) => {
                        setRouteToEdit(route);
                        graph?.$(':selected').unselect();
                        console.log(`selected ${route} with data ${JSON.stringify(routes[route])}`)
                        for (const node of routes[route].nodes) {
                            graph?.getElementById(node).select();
                        }
                        for (const edge of routes[route].edges) {
                            graph?.getElementById(edge).select();
                        }
                    }
                    } requireSelection />
                    <label style={{fontSize: 14, marginLeft: 10}}>
                        Color: 
                        <select style={{marginLeft: 5}} value={routeColor} onChange={e => {
                            setRouteColor(e.currentTarget.value);
                            console.log(`Setting route color to ${e.currentTarget.value}`);
                        }}>
                            <option value="orange">Orange</option>
                            <option value="yellow">Yellow</option>
                            <option value="lightgreen">Green</option>
                            <option value="blue">Blue</option>
                            <option value="red">Red</option>
                        </select>
                    </label>

                </div>
                <div>
                    <input type="text" value={newRouteName} onChange={e => setNewRouteName(e.currentTarget.value)}></input>
                    <button onClick={() => {
                        const id = (Math.floor(Math.random() * 2 ** 50)).toString();
                        setRoutes({ ...routes, [id]: { name: newRouteName, id, nodes: [], edges: [], color: routeColor } });
                        setNewRouteName("");
                    }}>Create route</button>
                </div>
            </> : null}
        </div>
    )
}

interface GraphPropertyInputProps {
    type: string,
    value: string,
    onChange: (value: string) => void,
    onSubmit: () => void,
    onCancel: () => void,
    x: number,
    y: number,

}

const GraphPropertyInput = ({ type, value, onChange, onSubmit, onCancel: onClose, x, y }: GraphPropertyInputProps) => (

    <input
        type={type}
        autoFocus
        value={value}
        onChange={e => onChange(e.currentTarget.value)}
        onBlur={() => {
            onSubmit();
        }}
        onKeyDown={e => {
            if (e.key === 'Enter') {
                e.currentTarget.blur();
            } else if (e.key === 'Escape') {
                onClose();
            }
            // TODO: fix document key handling doing things to edit boxes without this hack
            e.stopPropagation();
        }}
        style={{ position: 'absolute', width: 50, left: x, right: y }}
    />
);

function initializeGraph(core: Core, subwayGraph: SubwayGraph) {
    for (const node of subwayGraph.nodes) {
        core.add({ group: 'nodes', data: { id: node.id, name: node.name }, position: node.position });
    }
    for (const edge of subwayGraph.edges) {
        core.add({ group: 'edges', data: { ...edge } })
    }
}

function graphToSubwayGraph(core: Core, routes: SubwayGraph["routes"]): SubwayGraph {
    const nodes = [];
    for (const node of core.nodes()) {
        if (node.data().type !== 'train' && node.indegree(false) + node.outdegree(false) !== 0) {
            nodes.push({
                id: node.id(),
                name: node.data().name,
                position: node.position(),
            });
        }
    }
    const edges = [];
    for (const edge of core.edges()) {
        edges.push({
            id: edge.id(),
            weight: parseInt(edge.data().weight),
            type: edge.data().type,
            source: edge.source().id(),
            target: edge.target().id(),
        });
    }
    return {
        nodes,
        edges,
        routes,
    }
}

function clearTrains(graph: cytoscape.Core) {
    graph.$('node[type = "train"]').remove();
}

function setTrainPositions(graph: cytoscape.Core, simulationResults: SimulationResults, trainPositions: TrainPositions, routes: Routes) {
    clearTrains(graph);
    for (const train of trainPositions.trains) {
        const element = graph.$id(train.curr_section) as NodeSingular | EdgeSingular;
        let x = 0;
        let y = 0;
        if (element.isNode()) {
            x = element.position().x;
            y = element.position().y;
        } else if (element.isEdge()) {
            const sourcePos = element.source().position();
            const targetPos = element.target().position();
            x = sourcePos.x + (train.pos / element.data().weight) * (targetPos.x - sourcePos.x);
            y = sourcePos.y + (train.pos / element.data().weight) * (targetPos.y - sourcePos.y);
        } else {
            continue;
        }
        console.log(train.id.toString(), simulationResults.train_to_route[train.id.toString()], routes[simulationResults.train_to_route[train.id.toString()]]);
        graph.add({
            group: 'nodes',
            data: { type: 'train', name: '', color: routes[simulationResults.train_to_route[`${train.id[0]}_${train.id[1]}`]].color },
            position: {
                x,
                y,
            }
        });
    }
}

function renderTrainPositions(graph: cytoscape.Core, simulationResults: SimulationResults, routes: Routes) {
    function impl(graph: cytoscape.Core, pos: number) {
        if (pos >= simulationResults.train_positions.length) {
            setTimeout(() => clearTrains(graph));
            return;
        }
        setTimeout(() => {
            setTrainPositions(graph, simulationResults, simulationResults.train_positions[pos], routes);
            impl(graph, pos + 1);
        }, 1000);
    }

    impl(graph, 0);
}

function serializeGraph(graph: cytoscape.Core): any {
    return {
        nodes: graph.nodes().map(node => ({ id: node.id() })),
        edges: graph.edges().map(edge => ({ id: edge.id(), source: edge.source().id(), target: edge.target().id(), weight: edge.data().weight, type: edge.data().type }))
    };
}

const StationStatisticTooltip = ({ statistic, routes }: { statistic: StationStatistic, routes: Record<string, Route> }) => {
    return (
        <div>
            {
                Object.entries(statistic.arrival_times)
                    .map(([id, data]) => (
                        <div>
                            <b>{routes[id].name}</b>
                            <div>Average: {data.average_wait}</div>
                            <div>Min: {data.min_wait}</div>
                            <div>Max: {data.max_wait}</div>
                        </div>)
                    )
            }
            {statistic.overall_arrival_times ? (
                <div>
                    <b>Overall</b>
                    <div>Average: {statistic.overall_arrival_times.average_wait}</div>
                    <div>Min: {statistic.overall_arrival_times.min_wait}</div>
                    <div>Max: {statistic.overall_arrival_times.max_wait}</div>
                </div>) : null
            }
        </div>
    )
}

function trainPositionsToStringlines(positions: TrainPositions[], trainToRoute: Record<number, string>, to: number, routes: Set<string>, allRoutes: Routes): Record<number, Stringline[]> {
    const trainPositions: Record<string, StringlinePoint[]> = {};
    for (const position of positions) {
        if (position.time > to) break;
        for (const train of position.trains) {
            const trainKey = `${train.id[0]}_${train.id[1]}`;
            trainPositions[trainKey] ||= [];
            if (train.pos < 0) {
                console.log("detected train with negative position!");
            }
            trainPositions[trainKey].push({ x: position.time, y: train.pos + train.distance_travelled });
            train.curr_section
        }
    }

    const stringlines: Record<string, Stringline[]> = {}
    for (const route of routes) {
        stringlines[route] = [];
    }

    for (const [id, positions] of Object.entries(trainPositions)) {
        const route = trainToRoute[id as any];
        if (routes.has(route)) {
            stringlines[route].push({points: positions, color: allRoutes[route].color});
        }
    }

    return stringlines;
}

// TODO: implement
function namedStationsOfRoute(route: Route, graph: SubwayGraph): Station[] {
    const stations: Station[] = [];
    return stations;
}

type RouteSelectorProps = {
    route: string,
    setRoute: (r: string) => void,
    routes: Routes,
    requireSelection: true,
    label?: string,
} | {
    route: string | undefined,
    setRoute: (r: string | undefined) => void,
    routes: Routes,
    requireSelection?: false
    label?: string,
}

const RouteSelector: React.FC<RouteSelectorProps> = ({ route, setRoute, routes, requireSelection, label }) => {
    return (
    <label style={{fontSize: 14}}>
        {label ?? ''}
        <select style={{marginLeft: label ? 5 : 0}} value={route} onChange={e => setRoute(e.currentTarget.value)}>
            {requireSelection ? null : <option key="none">None</option>}
            {Object.entries(routes).map(([key, value]) => <option key={key} value={key}>{value.name}</option>)}
        </select>
    </label>
    )
}

function currentCenter(graph: cytoscape.Core): { x: number, y: number } {
    // Get viewport size
    const viewportWidth = graph.width();
    const viewportHeight = graph.height();

    // Get the current pan position
    const pan = graph.pan();

    // Get the current zoom level
    const zoom = graph.zoom();

    // Calculate the center coordinates in logical space
    const x = (viewportWidth / 2 - pan.x) / zoom;
    const y = (viewportHeight / 2 - pan.y) / zoom;
    return { x, y };
}