import { formatHermesMemoryDemo } from "./hermesMemoryDemoFormatter";
import { runHermesMemoryDemo } from "./hermesMemoryDemo";

const output = formatHermesMemoryDemo(runHermesMemoryDemo());

console.log(output);
