/**
 * Write the cross-language golden fixture `fixtures/presence-v0.json` from the pure
 * fixture data in `src/presenceFixtures.ts` (which is built from the canonical TS
 * factories). This script holds the only Node I/O, so it stays out of the tsc graph.
 *
 * Regenerate:  npm run gen:fixtures
 */
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { serializeFixtures } from "../src/presenceFixtures";

const FIXTURE_PATH = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "fixtures",
  "presence-v0.json",
);

writeFileSync(FIXTURE_PATH, serializeFixtures());
// eslint-disable-next-line no-console
console.log(`wrote ${FIXTURE_PATH}`);
