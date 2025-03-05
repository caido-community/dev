import fs from "fs/promises";
import { createServer } from "http";
import path from "path";

import { watch as chokidarWatch } from "chokidar";
import express, { type Request, type Response } from "express";
import { type WebSocket, WebSocketServer } from "ws";

import { loadConfig } from "../config";
import {
  type ConnectedMessage,
  type ErrorMessage,
  type RebuildMessage,
} from "../types";
import { logError, logInfo } from "../utils";

import { build } from "./build";

export async function watch(options: { path?: string; config?: string }) {
  const { path: cwd = process.cwd(), config: configPath } = options;

  const config = await loadConfig(cwd, configPath);
  const { port = 3000 } = config.watch ?? {};

  const downloadUrl = `http://localhost:${port}/plugin_package.zip`;

  const app = express();
  const server = createServer(app);
  const wss = new WebSocketServer({ server });

  // Keep track of connected clients
  const clients = new Set<WebSocket>();

  wss.on("connection", (ws: WebSocket) => {
    clients.add(ws);
    ws.on("close", () => clients.delete(ws));

    const message: ConnectedMessage = {
      kind: "connected",
      packageId: config.id,
      downloadUrl,
    };

    ws.send(JSON.stringify(message));
  });

  // Function to notify all clients of a rebuild
  const notifyRebuild = () => {
    const message: RebuildMessage = {
      kind: "rebuild",
      downloadUrl,
    };
    clients.forEach((client) => client.send(JSON.stringify(message)));
  };

  // Function to notify all clients of an error
  const notifyError = (error: string) => {
    const message: ErrorMessage = {
      kind: "error",
      error,
    };

    clients.forEach((client) => client.send(JSON.stringify(message)));
  };

  // Serve the plugin package
  app.get("/plugin_package.zip", async (req: Request, res: Response) => {
    const pluginPath = path.join(cwd, "dist", "plugin_package.zip");
    try {
      await fs.access(pluginPath);
      res.download(pluginPath);
    } catch {
      res.status(404).send("Plugin package not found");
    }
  });

  // Initial build
  try {
    await build(options);
    notifyRebuild();
  } catch (error) {
    if (error instanceof Error) {
      logError(error.message);
      notifyError(error.message);
    } else {
      logError("Unknown error");
      notifyError("Unknown error");
    }
  }

  // Watch for changes
  const watchPatterns = config.plugins.map((plugin) =>
    path.join(cwd, plugin.root, "**/*"),
  );
  const watchFiles = await Promise.all(
    watchPatterns.map(async (pattern) => {
      const files = [];
      for await (const file of fs.glob(pattern)) {
        files.push(file);
      }
      return files;
    }),
  );
  const filesToWatch = [
    ...watchFiles.flat(),
    path.join(cwd, "package.json"),
    path.join(cwd, "caido.config.ts"),
  ];
  const watcher = chokidarWatch(filesToWatch, {
    ignoreInitial: true,
    ignored: (f) => f.includes("dist/") || f.includes("node_modules/"),
  });

  watcher.on("all", async (event: string, filePath: string) => {
    logInfo(`File ${filePath} has been ${event}`);
    try {
      await build(options);
      notifyRebuild();
    } catch (error) {
      notifyError(error instanceof Error ? error.message : "Unknown error");
    }
  });

  // Start the server
  server.listen(port, () => {
    logInfo(`Development server running at http://localhost:${port}`);
    logInfo(`WebSocket server running at ws://localhost:${port}`);
  });
}
