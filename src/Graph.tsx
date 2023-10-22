import { useRef, MutableRefObject, useEffect, useState } from "react";
import cytoscape, { Core, EdgeSingular, NodeSingular } from "cytoscape";
import { SubwayGraph } from "./subwayGraph";

export type GraphMode = 'edit' | 'path_select' | 'display'

type GraphProps = {
    initialSubwayGraph: SubwayGraph,
    mode: GraphMode,
    onShortestPath: (graph: any, source: string, target: string) => void,
    getCurrentSubwayGraph?: MutableRefObject<() => SubwayGraph>,
}

// escape -> clear selection, clear state 
// n -> new node 
// select a, e, select b -> edge between a and b
// select node or edge, delete -> delete the selected item 


type EditType = { type: 'edgeCreate', edgeSourceNode: NodeSingular } | { type: 'edgeWeight', edge: EdgeSingular, renderedX: number, renderedY: number, weight: string } | { type: 'nodeName', node: NodeSingular, renderedX: number, renderedY: number, name: string } | { type: 'none' };

export const Graph = ({ initialSubwayGraph, mode, onShortestPath, getCurrentSubwayGraph }: GraphProps) => {
    const divRef = useRef(null);
    const [graph, setGraph] = useState<Core | undefined>();
    const [editType, setEditType] = useState<EditType>({ type: 'none' });

    useEffect(() => {
        if (graph && getCurrentSubwayGraph) {
            getCurrentSubwayGraph.current = () => graphToSubwayGraph(graph);
        }
    }, [graph, getCurrentSubwayGraph]);

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
                    case 'e':
                        if (selected.isNode()) {
                            setEditType({ type: 'edgeCreate', edgeSourceNode: selected })
                        }
                        break;
                    default:
                }

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
                        selector: 'node',
                        style: {
                            'label': 'data(name)',
                            'text-valign': 'center',
                            'text-halign': 'center',
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
                            graph.add({ data: { source: editType.edgeSourceNode.id(), target: target.id(), weight: parseInt(prompt("Edge weight") || "3") } });
                        }
                    }
                } else if (mode === 'path_select') {
                    const selected = graph.$(':selected');
                    if (selected.length === 2) {
                        const g =
                            { nodes: graph.nodes().map(node => ({ id: node.id() })), edges: graph.edges().map(edge => ({ source: edge.source().id(), target: edge.target().id(), weight: edge.data().weight })) };
                        console.log(g)
                        console.log(selected[0].id(), selected[1].id)
                        onShortestPath(
                            g,
                            selected[0].id(),
                            selected[1].id()
                        );
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
            if (mode === 'path_select') {
                graph.selectionType('additive')
            } else {
                graph.selectionType('single')
            }
        }
    })

    return (
        <>
            <div ref={divRef} style={{ width: 400, height: 400 }}></div>
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

function graphToSubwayGraph(core: Core): SubwayGraph {
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
            source: edge.source().id(),
            target: edge.target().id(),
        });
    }
    return {
        nodes,
        edges
    }
}