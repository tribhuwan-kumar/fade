import "./App.css";
import Slider from "@/components/Slider";
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from '@tauri-apps/api/event'

type MonitorInfo = {
  id: string
  name: string
  brightness: number
}

function App() {
  const [monitors, setMonitors] = useState<MonitorInfo[]>([])
  const [name, setName] = useState<any>("")

  // useEffect(() => {
  invoke("watch_monitors");
  const unlisten = listen<MonitorInfo[]>("monitors-changed", (event) => {
    const updated = event.payload; 
    setMonitors(updated)
    setName(updated.map(n => n.name).toString())
  })
    // return () => { unlisten.then(f => f()) }
  // }, [])

  unlisten.then(f => f())

  const handleChange = async () => {
    await invoke("set_brightness", { id: "\\\\.\\DISPLAY1",  level: 10 });
  };

  handleChange();

  console.log("Event received!", monitors);
  console.log("id", name)

  return (
    <div className="container">
      {monitors.map(m => (
        <div key={m.id}>
          <h3>{m.name}</h3>
          <Slider
            minValue={-100}
            maxValue={100}
            centerValue={0}
          />
        </div>
        // <Slider 
        //   key={m.id} 
        //   minValue={-80}
        //   maxValue={100}
        //   centerValue={0}
        // />
      ))}
    </div>
  );
}

export default App;
