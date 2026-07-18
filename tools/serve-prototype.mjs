import http from "node:http";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";

const root = path.resolve("designs/siaocut-workbench");
const mime = { ".html": "text/html; charset=utf-8", ".css": "text/css; charset=utf-8", ".js": "text/javascript; charset=utf-8" };
const port = Number(process.env.PORT || 4311);
const server = http.createServer(async (request, response) => {
  const target = path.resolve(root, request.url === "/" ? "index.html" : `.${request.url}`);
  if (!target.startsWith(root)) { response.writeHead(403); return response.end(); }
  try {
    await stat(target);
    response.writeHead(200, { "content-type": mime[path.extname(target)] || "application/octet-stream" });
    response.end(await readFile(target));
  } catch {
    response.writeHead(404); response.end("Not found");
  }
});
server.listen(port, "127.0.0.1", () => console.log(`SiaoCut prototype: http://127.0.0.1:${port}`));
