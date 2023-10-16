import { useRef, useEffect, useState } from "react";
import cytoscape, { Core, NodeSingular } from "cytoscape";

export type GraphMode = 'edit' | 'path_select' | 'display'

type GraphProps = {
    mode: GraphMode,
    onShortestPath: (graph: any, source: string, target: string) => void
}

// escape -> clear selection, clear state 
// n -> new node 
// select a, e, select b -> edge between a and b
// select node or edge, delete -> delete the selected item 

export const Graph = ({ mode, onShortestPath }: GraphProps) => {
    const divRef = useRef(null);
    const [graph, setGraph] = useState<Core | undefined>();
    // in edit mode, if an edge is being created, the source from which to make it 
    const [edgeSourceNode, setEdgeSourceNode] = useState<NodeSingular | undefined>(undefined);
    const [selected, setSelected] = useState([] as string[]);

    useEffect(() => {
        const handler = (event: KeyboardEvent) => {
            console.log('Responding to key', event.key);
            if (graph && mode === 'edit') {
                const selected =
                    graph.$(':selected')[0];
                switch (event.key) {
                    case 'Backspace':
                        selected.remove()
                        break;
                    case 'n':
                        graph.add({ data: {} })
                        break;
                    case 'e':
                        if (selected.isNode()) {
                            setEdgeSourceNode(selected);
                        }
                        break;
                    default:
                }

            }

        };
        document.addEventListener('keydown', handler);
        return () => document.removeEventListener('keydown', handler);
    }, [graph, mode])

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
            const handler = () => {
                if (mode === 'edit' && edgeSourceNode) {
                    const target = graph.$(':selected')[0]
                    if (target && target.isNode()) {
                        graph.add({ data: { source: edgeSourceNode.id(), target: target.id(), weight: parseInt(prompt("Edge weight") || "3")} });
                    }
                } else if (mode === 'path_select') {
                    const selected = graph.$(':selected');
                    if (selected.length === 2) {
                        const g = 
                            {nodes: graph.nodes().map(node => ({id: node.id()})), edges: graph.edges().map(edge => ({source: edge.source().id(), target: edge.target().id(), weight: edge.data().weight}))}; 
                        console.log(g)
                        console.log(selected[0].id(), selected[1].id)
                        onShortestPath(
                            g,
                            selected[0].id(), 
                            selected[1].id()
                        );
                    }
                }
                setEdgeSourceNode(undefined)
            };
            graph.on('select', handler);
            return () => graph.removeListener('select', handler);
        }
    }, [mode, edgeSourceNode, onShortestPath])
    
    useEffect(() => {
        if (graph) {
            if (mode === 'path_select') {
                graph.selectionType('additive')
            } else {
                graph.selectionType('single')
            }
        }
    })

    return <div ref={divRef} style={{ width: 400, height: 400 }}></div>
}