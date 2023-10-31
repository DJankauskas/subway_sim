import { useRef, MutableRefObject, useEffect, useState } from "react";
import cytoscape, { Core, EdgeSingular, NodeSingular } from "cytoscape";
import { SubwayGraph } from "./subwayGraph";

export type GraphMode = 'edit' | 'path_select' | 'route_edit' | 'display'

export interface TrainPositions {
    time: number,
    trains: TrainPosition[]
}

export interface TrainPosition {
    id: number,
    curr_section: string,
    pos: number,
}

type GraphProps = {
    initialSubwayGraph: SubwayGraph,
    mode: GraphMode,
    onShortestPath: (graph: any, source: string, target: string) => void,
    onSimulate: (graph: any, routes: any) => Promise<TrainPositions[]>,
    getCurrentSubwayGraph?: MutableRefObject<() => SubwayGraph>,
}

// escape -> clear selection, clear state 
// n -> new node 
// select a, e, select b -> edge between a and b
// select node or edge, delete -> delete the selected item 


type EditType = { type: 'edgeCreate', edgeSourceNode: NodeSingular } | { type: 'edgeWeight', edge: EdgeSingular, renderedX: number, renderedY: number, weight: string } | { type: 'nodeName', node: NodeSingular, renderedX: number, renderedY: number, name: string } | { type: 'none' };

export const Graph = ({ initialSubwayGraph, mode, onSimulate, onShortestPath, getCurrentSubwayGraph }: GraphProps) => {
    const divRef = useRef(null);
    const [graph, setGraph] = useState<Core | undefined>();
    const [editType, setEditType] = useState<EditType>({ type: 'none' });
    const [routes, setRoutes] = useState<SubwayGraph["routes"]>({});

    useEffect(() => {
        if (graph && getCurrentSubwayGraph) {
            getCurrentSubwayGraph.current = () => graphToSubwayGraph(graph, routes);
        }
    }, [graph, getCurrentSubwayGraph, routes]);

    useEffect(() => {
        const keydownHandler = (event: KeyboardEvent) => {
            console.log('Responding to key', event.key);
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
                        graph.add({ data: { name: '' } })
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
                    console.log('Enter pressed!')
                    const selected = graph!.$(':selected');
                    const route = {
                        name: 'TODO',
                        id: (Math.floor(Math.random() * 2 ** 50)).toString(),
                        nodes: [] as string[],
                        edges: [] as string[],
                    }
                    selected.filter('node').forEach(node => { route.nodes.push(node.id()) });
                    selected.filter('edge').forEach(edge => { route.edges.push(edge.id()) });
                    selected.unselect();
                    setRoutes({ ...routes, [route.id]: route });
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
    }, [graph, mode, editType])

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
                            'background-color': 'red'
                        }
                    }
                ]
            });
            setGraph(graph);
        }
    }, [divRef.current])

    useEffect(() => {
        if (graph && initialSubwayGraph) {
            initializeGraph(graph, initialSubwayGraph);
            setRoutes(initialSubwayGraph.routes);
            graph.reset();
        }
        return () => { graph?.elements().remove(); }
    }, [graph, initialSubwayGraph])

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
                        console.log(g)
                        console.log(selected[0].id(), selected[1].id)
                        onShortestPath(
                            g,
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
            const nodeDblclickHandler = (event: any) => {
                const currentNode = event.target;
                const { x, y } = event.renderedPosition;
                setEditType({ type: 'nodeName', node: currentNode, renderedX: x, renderedY: y, name: currentNode.data("name") })
            }
            graph.on('select', selectHandler);
            graph.on('dblclick', 'edge', edgeDblclickHandler);
            graph.on('dblclick', 'node', nodeDblclickHandler);
            return () => {
                graph.removeListener('select', selectHandler)
                    .removeListener('dblclick', edgeDblclickHandler)
                    .removeListener('dblclick', nodeDblclickHandler);
            }
        }
    }, [mode, editType, onShortestPath])

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
        <>
            <div ref={divRef} style={{ width: 800, height: 400 }}></div>
            <button onClick={async () => {
                if (graph) {
                    const results = await onSimulate(serializeGraph(graph), routes);
                    renderTrainPositions(graph, results);
                }
            }}>Simulate</button>
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
        </>
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
        nodes.push({
            id: node.id(),
            name: node.data().name,
            position: node.position(),
        });
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

function setTrainPositions(graph: cytoscape.Core, trainPositions: TrainPositions) {
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
            console.log(train);
            console.log(element);
        }
        graph.add({
            group: 'nodes',
            data: { type: 'train' },
            position: {
                x,
                y,
            }
        });
    }
}

function renderTrainPositions(graph: cytoscape.Core, trainPositions: TrainPositions[]) {
    function impl(graph: cytoscape.Core, trainPositions: TrainPositions[], pos: number) {
        if (pos >= trainPositions.length) {
            setTimeout(() => clearTrains(graph));
            return;
        }
        setTimeout(() => {
            setTrainPositions(graph, trainPositions[pos]);
            impl(graph, trainPositions, pos + 1);
        }, 1000);
    }

    impl(graph, trainPositions, 0);
}

function serializeGraph(graph: cytoscape.Core): any {
    return {
        nodes: graph.nodes().map(node => ({ id: node.id() })),
        edges: graph.edges().map(edge => ({ id: edge.id(), source: edge.source().id(), target: edge.target().id(), weight: edge.data().weight }))
    };
}