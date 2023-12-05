import { useCallback, useRef, useState } from "react";

import { invoke } from "@tauri-apps/api/tauri";
import {open, save} from "@tauri-apps/api/dialog";
import {readTextFile, writeTextFile} from "@tauri-apps/api/fs";

import { Graph, GraphMode, SimulationResults } from "./Graph";
import {SubwayGraph, defaultSubwayGraph} from "./subwayGraph";
import "./App.css";

interface ShortestPath {
  length: number,
  path: string[]
}

async function shortestPath(graph: any, source: string, target: string): Promise<ShortestPath | null>  {
  const result = await invoke('shortest_path', {jsGraph: graph, source, target});
  alert(JSON.stringify(result))
  return result as any;
}


async function runSimulation(graph: any, routes: any, frequency: number): Promise<SimulationResults> {
  const result = await invoke('run_simulation', {jsGraph: graph, jsRoutes: routes, frequency});
  console.log(result);
  return result as any;
}


function App() {
  const [mode, setMode] = useState<GraphMode>('display');
  const getSubwayGraph = useRef(defaultSubwayGraph);
  const [initialSubwayGraph, setInitialSubwayGraph] = useState(defaultSubwayGraph());
  const handleMode = useCallback((event: any) => {
    setMode(event.currentTarget.value)
  }, []);
  return (
    <div style={{display: 'flex', flexDirection: 'column'}}>
    <h1>Shortest Path</h1>
      <Graph mode={mode} initialSubwayGraph={initialSubwayGraph} onSimulate={runSimulation} onShortestPath={shortestPath} getCurrentSubwayGraph={getSubwayGraph} />
      <div>
        <div>
          <input type="radio" value="display" checked={mode === "display"} onChange={handleMode} />
          <label htmlFor="display">View</label>
        </div>
        <div>
          <input type="radio" value="edit" checked={mode === "edit"} onChange={handleMode} />
          <label htmlFor="edit">Edit</label>
        </div>
        <div>
          <input type="radio" value="path_select" checked={mode === "path_select"} onChange={handleMode} />
          <label htmlFor="path_select">Shortest path</label>
        </div>
        <div>
          <input type="radio" value="route_edit" checked={mode === "route_edit"} onChange={handleMode} />
          <label htmlFor="route_edit">Route creation</label>
        </div>
      </div>
      <div>
        <button onClick={async () => {
          const filePath = await open({
            filters: [{name: "Subway Graph", extensions: ["json"]}]
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
        }}>Load</button>
        <button onClick={async() => {
          const filePath = await save({filters: [{name: "Subway Graph", extensions: ["json"]}]});
          if (typeof filePath === 'string') {
            const subwayGraph = getSubwayGraph.current();
            await writeTextFile(filePath, JSON.stringify(subwayGraph));
          }
        }}>Save</button>
      </div>
    </div>
  )
}

export default App;
