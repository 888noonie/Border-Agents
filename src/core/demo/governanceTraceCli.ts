import { runHermesMemoryDemo } from "./hermesMemoryDemo";
import { buildTrace, formatTrace } from "../governanceTrace";

const output = runHermesMemoryDemo()
  .map((result) => formatTrace(buildTrace({ frame: result.frame, prompt: result.prompt })))
  .join("\n\n---\n\n");

console.log(output);
