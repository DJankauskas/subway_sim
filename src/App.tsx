import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { Graph, GraphMode } from "./Graph";
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

function App() {
  const [mode, setMode] = useState<GraphMode>('display');
  const handleMode = useCallback((event: any) => {
    setMode(event.currentTarget.value)
  }, []);
  return (
    <div>
    <h1>Shortest Path</h1>
      <Graph mode={mode} onShortestPath={shortestPath} />
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
          <label htmlFor="path select">Shortest path</label>
        </div>
      </div>
    </div>
  )
}

export default App;
