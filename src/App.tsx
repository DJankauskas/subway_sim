import { useCallback, useRef, useState } from "react";

import { invoke } from "@tauri-apps/api/tauri";
import { open, save } from "@tauri-apps/api/dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/api/fs";

import { Graph, GraphMode, SimulationResults } from "./Graph";
import { Routes, SubwayGraph, defaultRoutes, defaultSubwayGraph } from "./subwayGraph";
import "./App.css";

interface ShortestPath {
  length: number,
  path: string[]
}

async function shortestPath(graph: any, routes: any, source: string, target: string): Promise<ShortestPath | null> {
  const result = await invoke('shortest_path', { jsGraph: graph, jsRoutes: routes, source, target });
  alert(JSON.stringify(result))
  return result as any;
}


async function runSimulation(graph: any, routes: any, frequency: number): Promise<SimulationResults> {
  const result = await invoke('run_simulation', { jsGraph: graph, jsRoutes: routes, frequency });
  console.log(result);
  return result as any;
}

async function runOptimize(graph: any, routes: any): Promise<SimulationResults> {
  const result = await invoke('run_optimize', { jsGraph: graph, jsRoutes: routes });
  console.log(result);
  return result as any;
}


function App() {
  const [mode, setMode] = useState<GraphMode>('display');
  const getSubwayGraph = useRef(defaultSubwayGraph);
  const [initialSubwayGraph, setInitialSubwayGraph] = useState(defaultSubwayGraph());

  const getRoutes = useRef(defaultRoutes);
  const [initialRoutes, setInitialRoutes] = useState(defaultRoutes());

  const handleMode = useCallback((event: any) => {
    setMode(event.currentTarget.value)
  }, []);
  return (
    <div style={{ display: 'flex', flexDirection: 'column' }}>
      <Graph mode={mode} initialSubwayGraph={initialSubwayGraph} initialRoutes={initialRoutes} onSimulate={runSimulation} onOptimize={runOptimize} onShortestPath={shortestPath} getCurrentSubwayGraph={getSubwayGraph} getCurrentRoutes={getRoutes} />
      <div>
        <div>
          <input type="radio" value="display" checked={mode === "display"} onChange={handleMode} />
          <label htmlFor="display">View</label>
        </div>
        <div>
          <input type="radio" value="edit" checked={mode === "edit"} onChange={handleMode} />
          <label htmlFor="edit">Edit</label>
        </div>
        <div hidden={true}>
          <input type="radio" value="path_select" checked={mode === "path_select"} onChange={handleMode} />
          <label htmlFor="path_select">Shortest path</label>
        </div>
        <div>
          <input type="radio" value="route_edit" checked={mode === "route_edit"} onChange={handleMode} />
          <label htmlFor="route_edit">Route editing</label>
        </div>
      </div>
      <div>
        <div>
          <button onClick={async () => {
            const filePath = await open({
              filters: [{ name: "Subway Graph", extensions: ["json"] }]
            });
            if (typeof filePath === 'string') {
              const rawData = JSON.parse(await readTextFile(filePath));
              rawData.nodes = rawData.nodes.filter((node: any) => typeof node.name === 'string');
              console.log(rawData);
              const subwayGraphResult = SubwayGraph.safeParse(rawData);
              if (!subwayGraphResult.success) {
                console.error(subwayGraphResult.error);
                return;
              }
              setInitialSubwayGraph(subwayGraphResult.data);
            }
          }}>Load Layout</button>
          <button onClick={async () => {
            const filePath = await save({ filters: [{ name: "Subway Graph", extensions: ["json"] }] });
            if (typeof filePath === 'string') {
              const subwayGraph = getSubwayGraph.current();
              await writeTextFile(filePath, JSON.stringify(subwayGraph));
            }
          }}>Save Layout</button>
        </div>
        <div>
          <button onClick={async () => {
            const filePath = await open({
              filters: [{ name: "Subway Routes", extensions: ["json"] }]
            });
            if (typeof filePath === 'string') {
              const rawData = JSON.parse(await readTextFile(filePath));
              const routesResult = Routes.safeParse(rawData);
              if (!routesResult.success) {
                console.error(routesResult.error);
                return;
              }
              setInitialRoutes(routesResult.data);
            }
          }}>Load Routes</button>
          <button onClick={async () => {
            const filePath = await save({ filters: [{ name: "Subway Routes", extensions: ["json"] }] });
            if (typeof filePath === 'string') {
              const routes = getRoutes.current();
              await writeTextFile(filePath, JSON.stringify(routes));
            }
          }}>Save Routes</button>
        </div>
      </div>
    </div>
  )
}

export default App;
