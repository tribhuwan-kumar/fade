import "./App.css";
import Slider from "@/components/Slider";
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from '@tauri-apps/api/event'

type MonitorInfo = {
  name: string
  device_name: string
  brightness: number
}

function App() {
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);

  useEffect(() => {
    invoke("watch_monitors");
    const unlisten = listen<MonitorInfo[]>("monitors-changed", (event) => {
      const updated = event.payload; 
      setMonitors(updated)
    })

    return () => { unlisten.then(f => f()) }
  }, [])

  const handleSlider = async (deviceName: string, value: number) => {
    try {
      await invoke("set_brightness", { deviceName: deviceName, value: value });
      setMonitors((prev) =>
        prev.map((m) =>
          m.device_name === deviceName ? { ...m, brightness: value } : m
        )
      );
    } catch (e) {
      console.error("failed to set brightness:", e);
    }
  };

  const handleReset = async (deviceName: string, value: number) => {
    await handleSlider(deviceName, value);
  };

  return (
    <div className="container">
      {monitors.map(m => (
        <Slider
          onChange={(val: number) => handleSlider(m.device_name, val)}
          onDoubleClick={() => handleReset(m.device_name, 0)}
          key={m.device_name}
          minValue={-80}
          maxValue={100}
          centerValue={0}
          brightnessValue={m.brightness}
        />
      ))}
    </div>
  );
}

export default App;
