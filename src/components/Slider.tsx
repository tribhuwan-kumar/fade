import React, { 
  useState,
  useEffect,
  useMemo
} from "react";

interface SliderProps {
  minValue: number;
  maxValue: number;
  brightnessValue: number,
  centerValue?: number;
  className?: string;
  onChange: (v: number) => Promise<void>;
  onDoubleClick: () => Promise<void>;
}

const Slider: React.FC<SliderProps> = ({
  minValue,
  maxValue,
  brightnessValue,
  centerValue,
  className,
  onChange,
  onDoubleClick,
}) => {
  const center = useMemo(
    () =>
      centerValue !== undefined
        ? centerValue 
        : (minValue + maxValue) / 2,
    [minValue, maxValue, centerValue]
  );

  const [value, setValue] = useState(brightnessValue);

  const valuePercent = useMemo(
    () => ((value - minValue) / (maxValue - minValue)) * 100,
    [value, minValue, maxValue]
  );
  
  const centerPercent = useMemo(
    () => ((center - minValue) / (maxValue - minValue)) * 100,
    [center, minValue, maxValue]
  );

  const fillStyle = useMemo(() => {
    if (valuePercent >= centerPercent) {
      return {
        left: `${centerPercent}%`,
        width: `${valuePercent - centerPercent}%`,
      };
    } else {
      return {
        left: `${valuePercent}%`,
        width: `${centerPercent - valuePercent}%`,
      };
    }
  }, [valuePercent, centerPercent]);

  // thumb width
  const thumbStyle = useMemo(
    () => ({
      left: `calc(${valuePercent}% - 8px)`,
    }),
    [valuePercent]
  );

  const tooltipStyle = useMemo(
    () => ({
      left: `${valuePercent}%`,
      transform: 'translateX(-50%)',
    }),
    [valuePercent]
  );

  useEffect(() => {
    setValue(brightnessValue)
  }, [brightnessValue])

  return (
    <div
      className={`w-[90%] mt-[45px] mr-auto ml-auto my-auto flex items-center justify-center ${ className || ""}`}
    >
      <div className="relative w-full h-fit inline-block">
        <div className="group w-full" onDoubleClick={onDoubleClick}>
          <div
            style={tooltipStyle}
            className="absolute bottom-full mb-4 px-1 py-0.5 bg-[#424242] text-slate-200/80 text-sm rounded-md opacity-0 group-hover:opacity-100
            shadow-lg transition-opacity duration-300 pointer-events-none"
          >
            <span className="font-medium text-center">{value}</span>
            <div className="absolute top-full left-1/2 -translate-x-1/2 w-0 h-0 border-x-4 border-x-transparent border-t-4 border-t-[#424242]"></div>
          </div>
          <div className="relative flex h-[5px] items-center">
            <div className="absolute w-full h-full rounded-full bg-gray-300"></div>
            <div
              className="absolute h-[5px] rounded-full bg-[#514e4c]"
              style={fillStyle}
            />
            <div
              className="absolute top-1/2 -translate-y-1/2 w-4 h-4 bg-white rounded-full cursor-pointer shadow-md border-2 
              border-gray-600/70 group-hover:scale-110 transition-transform duration-200"
              style={thumbStyle}
            />
          </div>
          <input
            type="range"
            min={minValue}
            max={maxValue}
            value={value}
            onChange={(e) => onChange(Number(e.target.value))}
            className="absolute w-full h-[5px] top-0 left-0 opacity-0 cursor-pointer"
          />
        </div>
        <div className="flex justify-between text-xs text-slate-200/60 mt-3">
          <span>{minValue}</span>
          <span
            className="absolute"
            style={{
              left: `${centerPercent}%`,
              transform: 'translateX(-50%)'
            }}
          >
            {center}
          </span>
          <span>{maxValue}</span>
        </div>
      </div>
    </div>
  );
};

export default Slider;
