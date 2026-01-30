import "./App.css";
import fubuki from "@/assets/fubuki.jpg";
import Slider from "@/components/Slider";
import { invoke } from "@tauri-apps/api/core";
import { useState, useEffect, useRef } from "react";
import { LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";

const WINDOW_WIDTH = 380;

type MonitorInfo = {
  /// unique identifier
  device_name: string
  /// actual monitor name same as settings
  name: string
  /// brightness value
  brightness: number
}

function App() {
  const [errors, setErrors] = useState<Array<String>>([]);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const socket = new WebSocket('ws://127.0.0.1:8956/ws/monitors');

    socket.onopen = () => {
      console.log("connected to websocket");
    };

    socket.onmessage = (event) => {
      try {
        const monitors = JSON.parse(event.data);
        setMonitors(monitors);
        console.log(monitors);
      } catch (err) {
        setErrors(prev => [...prev, (err as Error)?.message || String(err)]);
        console.error("failed to parse monitor data", err);
      }
    };

    socket.onerror = (err: Event | Error) =>
      setErrors(prev => [...prev, (err as Error)?.message || String(err)]);

    return () => {
      socket.close();
    };
  }, []); 

  useEffect(() => {
    if (containerRef.current) {
      const contentHeight = containerRef.current.scrollHeight;
      const newHeight = contentHeight + 70;
      const win = getCurrentWindow();
      win.setSize(new LogicalSize(WINDOW_WIDTH, newHeight));
    }
  }, [monitors]);


  const handleSlider = async (value: number, deviceName: string) => {
    try {
      await invoke("set_brightness", { value: value, deviceName: deviceName });
      setMonitors((prev) =>
        prev.map((m) =>
          m.device_name === deviceName ? { ...m, brightness: value } : m
        )
      );
    } catch (e) {
      console.error("failed to set brightness:", e);
    }
  };

  return (
    <main
      id="root"
      className="min-h-screen w-full bg-black/40 backdrop-blur-m flex justify-center flex-col items-center"
    >
      <div
        className="absolute inset-0 bg-cover bg-center"
        style={{
          backgroundImage: `url(${fubuki})`,
          opacity: 0.35,
        }}
      />
      <div 
        className="container"
        ref={containerRef}
      >
        {monitors.map(m => (
          <Slider
            displayName={m.name.toLowerCase()}
            onChange={(val: number) => handleSlider(val, m.device_name)}
            onDoubleClick={() => handleSlider(0, m.device_name)}
            key={m.device_name}
            minValue={-100}
            maxValue={100}
            centerValue={0}
            brightnessValue={m.brightness}
          />
        ))}
      </div>
    </main>
  );
}

export default App;
