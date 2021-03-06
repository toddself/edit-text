// Talk to sync server, or to client proxy.

import * as React from 'react';

import * as route from './route';
import * as app from './app';
import {EditorFrame} from './app';
import * as commands from '../editor/commands';
import {ServerImpl, ControllerImpl } from '../editor/network';
import {WasmClient, WasmError, getForwardWasmTaskCallback, setForwardWasmTaskCallback} from '../editor/wasm';
import DEBUG from '../debug';

class DeferredSocket {
  socket: WebSocket;

  openQueue: Array<any>;
  messageQueue: Array<any>;
  closeQueue: Array<any>;

  constructor(url: string) {
    this.openQueue = [];
    this.messageQueue = [];
    this.closeQueue = [];

    let self = this;
    this.socket = new WebSocket(
      route.syncUrl()
    );
    this.socket.onopen = function () {
      DEBUG.measureTime('websocket-defer-open');
      self.openQueue.push(arguments);
    };
    this.socket.onmessage = function () {
      self.messageQueue.push(arguments);
    };
    this.socket.onclose = function () {
      self.closeQueue.push(arguments);
    };
  }

  handle(handlers: any) {
    this.socket.onopen = handlers.onopen;
    this.socket.onmessage = handlers.onmessage;
    this.socket.onclose = handlers.onclose;

    for (let item of this.openQueue) {
      console.log('deferred open', item);
      this.socket.onopen!.apply(this.socket, item);
    }
    for (let item of this.messageQueue) {
      console.log('deferred message', item);
      this.socket.onmessage!.apply(this.socket, item);
    }
    for (let item of this.closeQueue) {
      console.log('deferred close', item);
      this.socket.onclose!.apply(this.socket, item);
    }
  }
}

let syncSocket = new DeferredSocket(
  route.syncUrl()
);

export class AppServer implements ServerImpl {
  client: WasmClient | null;
  
  onClose: () => void;

  private nativeSocket: WebSocket;

  // Create a deferred object for the sync socket
  // because we may receive ServerCommand payloads earlier
  private deferSync: Promise<WebSocket>;
  private deferSyncResolve: (socket: WebSocket) => void | null;

  private editorFrame: EditorFrame | null;

  constructor() {
    this.deferSync = new Promise((resolve, reject: any) => {
      this.deferSyncResolve = resolve;
    });
  }

  sendCommand(command: any) {
    return this.deferSync.then(syncSocket => {
      syncSocket.send(JSON.stringify(command));
    });
  }

  connect(onError: (message: React.ReactNode) => void): Promise<void> {
    let server = this;

    return Promise.resolve()
    .then(() => {
      DEBUG.measureTime('connect-server');

      syncSocket.handle({
        onopen: (event: any) => {
          console.debug('server socket opened.');
          DEBUG.measureTime('connect-ready');
        },
        onmessage: (event: any) => {
          // console.log('Got message from sync:', event.data);
          try {
            if (getForwardWasmTaskCallback() != null) {
              if (server.client != null) {
                server.client.clientBindings.command(JSON.stringify({
                  ClientCommand: JSON.parse(event.data),
                }));
              }
            }
          } catch (e) {
            // Kill the current process, we triggered an exception.
            setForwardWasmTaskCallback(null);
            if (server.client != null) {
              server.client.Module.wasm_close();
            }
            // syncSocket.close();

            // TODO this is the wrong place to put this
            (document as any).body.background = 'red';

            if (server.editorFrame) {
              onError(
                <div>The client experienced an error talking to the server and you are now disconnected. We're sorry. You can <a href="?">refresh your browser</a> to continue.</div>
              );
            }

            throw new WasmError(e, `Error during sync command: ${e.message}`);
          }
        },
        onclose: () => {
          if (server.editorFrame) { 
            onError(
              <div>The editor has disconnected from the server. We're sorry. You can <a href="?">refresh your browser</a>, or we'll refresh once the server is reachable.</div>
            );
          }

          setTimeout(() => {
            setInterval(() => {
              app.graphqlPage('home').then(() => {
                // Can access server, continue
                window.location.reload();
              });
            }, 2000);
          }, 3000);

          server.onClose();
        },
      });

      if (this.deferSyncResolve !== null) {
        this.deferSyncResolve(syncSocket.socket);
      }
    });
  }
}

export class ProxyClient implements ControllerImpl {
  // TODO shouldn't these be nullable?
  onMessage: (msg: any) => void | null;
  onClose: () => void | null;

  private editorID: string;

  private socket: WebSocket;

  sendCommand(command: commands.Command) {
    delete command.tag;
    this.socket.send(JSON.stringify(command));
  }

  connect(onError: () => void): Promise<void> {
    let network = this;
    return Promise.resolve()
    .then(() => {
      this.socket = new WebSocket(
        route.clientProxyUrl()
      );
      this.socket.onopen = function (event) {
        console.debug('client-proxy socket opened.');
      };
      this.socket.onmessage = function (event) {
        let parse = JSON.parse(event.data);
        if (network.onMessage !== null) {
          network.onMessage(parse);
        }
      };
      this.socket.onclose = network.onClose;
    });
  }
}
