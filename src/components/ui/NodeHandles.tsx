import { Handle, Position } from "@xyflow/react";

// Connection handles on all four sides (D18). Connections are undirected and
// drawn as ropes anchored on whichever side faces the other node, so a handle
// is only where a drag starts; every side has both kinds so old edges without
// a handle id still find one. Hidden until the node is hovered (index.css).
const SIDES: Array<[Position, string]> = [
  [Position.Top, "top"],
  [Position.Right, "right"],
  [Position.Bottom, "bottom"],
  [Position.Left, "left"],
];

export default function NodeHandles() {
  return (
    <>
      {SIDES.map(([pos, side]) => (
        <Handle key={`s-${side}`} id={side} type="source" position={pos} className="narrater-handle" />
      ))}
      {SIDES.map(([pos, side]) => (
        <Handle key={`t-${side}`} id={`${side}-in`} type="target" position={pos} className="narrater-handle" />
      ))}
    </>
  );
}
