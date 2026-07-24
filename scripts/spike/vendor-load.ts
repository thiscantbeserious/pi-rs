// Phase 0 vendor-path spike: load the real extension corpus under Deno using
// pi's own MIT-licensed loader (discoverAndLoadExtensions) from the pinned
// 0.82.0 npm package. Tests whether vendoring pi's runtime works unmodified.
//
// Run: deno run --allow-read --allow-write --allow-env --allow-sys --allow-net \
//        --minimum-dependency-age=0 scripts/spike/vendor-load.ts
// (--allow-write covers the two shimmable cases in ADR 0021;
//  --minimum-dependency-age=0 because the pinned 0.82.0 is <24h old at spike start)

import { discoverAndLoadExtensions } from "npm:@earendil-works/pi-coding-agent@0.82.0";

const home = Deno.env.get("HOME")!;
const agentDir = `${home}/.pi/agent`;
const extensionsDir = `${agentDir}/extensions`;

// pi discovers extensions from configured paths + the standard ~/.pi tree.
// configuredPaths here mirrors a typical interactive startup (the extensions dir).
const configuredPaths = [extensionsDir];

try {
	const result = await discoverAndLoadExtensions(
		configuredPaths,
		Deno.cwd(),
		agentDir,
	);
	console.log(`LOADED ${result.extensions.length} extension(s):`);
	for (const ext of result.extensions) {
		console.log(`  ok: ${ext.path}`);
	}
	console.log(`\nFAILED ${result.errors.length} extension(s):`);
	for (const e of result.errors) {
		console.log(`  FAIL: ${e.path}\n    ${e.error}`);
	}
	if (result.errors.length > 0) Deno.exit(1);
} catch (err) {
	console.error("FATAL: loader threw before returning a result");
	console.error(err);
	Deno.exit(2);
}
