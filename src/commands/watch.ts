import { createServer } from 'http';
import { WebSocket, WebSocketServer } from 'ws';
import express, { Request, Response } from 'express';
import { watch as chokidarWatch } from 'chokidar';
import { build } from './build';
import path from 'path';
import fs from 'fs/promises';
import { loadConfig } from '../config';
import { logInfo } from '../utils';

export async function watch(options: {
    path?: string;
    config?: string;
}) {

    const { path: cwd = process.cwd(), config: configPath } = options;
    const app = express();
    const server = createServer(app);
    const wss = new WebSocketServer({ server });
    
    // Keep track of connected clients
    const clients = new Set<WebSocket>();
    
    wss.on('connection', (ws: WebSocket) => {
        clients.add(ws);
        ws.on('close', () => clients.delete(ws));
    });

    // Function to notify all clients of a rebuild
    const notifyRebuild = (success: boolean, error?: string) => {
        const message = JSON.stringify({ type: 'rebuild', success, error });
        clients.forEach(client => client.send(message));
    };

    // Serve the plugin package
    app.get('/plugin_package.zip', async (req: Request, res: Response) => {
        const pluginPath = path.join(cwd, 'dist', 'plugin_package.zip');
        try {
            await fs.access(pluginPath);
            res.download(pluginPath);
        } catch {
            res.status(404).send('Plugin package not found');
        }
    });

    // Initial build
    try {
        await build(options);
        notifyRebuild(true);
    } catch (error) {
        notifyRebuild(false, error instanceof Error ? error.message : 'Unknown error');
    }

    // Watch for changes
    const config = await loadConfig(cwd, configPath);
    const watchPatterns = config.plugins.map(plugin => path.join(cwd, plugin.root, '**/*'));
    const watchFiles = await Promise.all(
        watchPatterns.map(async pattern => {
            const files = [];
            for await (const file of fs.glob(pattern)) {
                files.push(file);
            }
            return files;
        })
    );
    const filesToWatch = [
        ...watchFiles.flat(),
        path.join(cwd, 'package.json'),
        path.join(cwd, 'caido.config.ts'),
    ];
    const watcher = chokidarWatch(filesToWatch, {
        ignoreInitial: true,
        ignored: (f) => f.includes("dist/")
    });

    watcher.on('all', async (event: string, filePath: string) => {
        logInfo(`File ${filePath} has been ${event}`);
        try {
            await build(options);
            notifyRebuild(true);
        } catch (error) {
            notifyRebuild(false, error instanceof Error ? error.message : 'Unknown error');
        }
    });

    // Start the server
    const port = 3000;
    server.listen(port, () => {
        logInfo(`Development server running at http://localhost:${port}`);
        logInfo(`WebSocket server running at ws://localhost:${port}`);
    });
} 