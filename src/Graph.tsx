import { useRef, useEffect, useState } from "react";
import cytoscape, { Core, EdgeSingular, NodeSingular } from "cytoscape";

export type GraphMode = 'edit' | 'path_select' | 'display'

type GraphProps = {
    mode: GraphMode,
    onShortestPath: (graph: any, source: string, target: string) => void
}

// escape -> clear selection, clear state 
// n -> new node 
// select a, e, select b -> edge between a and b
// select node or edge, delete -> delete the selected item 


type EditType = { type: 'edgeCreate', edgeSourceNode: NodeSingular } | { type: 'edgeWeight', edge: EdgeSingular, renderedX: number, renderedY: number, weight: string } | { type: 'none' };

export const Graph = ({ mode, onShortestPath }: GraphProps) => {
    const divRef = useRef(null);
    const [graph, setGraph] = useState<Core | undefined>();
    const [editType, setEditType] = useState<EditType>({ type: 'none' });

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
                        graph.add({ data: {} })
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
            console.log('init cytoscape!')
            const graph = cytoscape({
                container: divRef.current,
                elements: [ // list of graph elements to start with
                    { // node a
                        data: { id: 'a' }
                    },
                    { // node b
                        data: { id: 'b' }
                    },
                    { // edge ab
                        data: { id: 'ab', source: 'a', target: 'b', weight: 5 }
                    }
                ],
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
                            'label': 'data(id)',
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
            const dblclickHandler = (event: any) => {
                const currentEdge = event.target;
                const { x, y } = event.renderedPosition;
                setEditType({type: 'edgeWeight', edge: currentEdge, renderedX: x, renderedY: y, weight: currentEdge.data("weight")})
            };
            graph.on('select', selectHandler);
            graph.on('dblclick', 'edge', dblclickHandler);
            return () => {
                graph.removeListener('select', selectHandler);
                graph.removeListener('dblclick, dblclickHandler')
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
                ? <input
                    type="number"
                    autoFocus
                    value={editType.weight}
                    onChange={e => setEditType({ ...editType, weight: e.currentTarget.value })}
                    onBlur={() => {
                        const parsedWeight = parseInt(editType.weight);
                        if (parsedWeight) {
                            editType.edge.data('weight', editType.weight);
                        }
                        setEditType({type: 'none'});
                    }}
                    onKeyDown={e => {
                        if (e.key === 'Enter') {
                            e.currentTarget.blur();
                        } else if (e.key === 'Escape') {
                            setEditType({type: 'none'})
                        }
                        // TODO: fix document key handling doing things to edit boxes without this hack
                        e.stopPropagation();
                    }}
                    style={{ position: 'absolute', width: 50, left: editType.renderedX, right: editType.renderedY }}
                />
                : null}
        </>
    )
}