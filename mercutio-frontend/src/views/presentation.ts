import * as commands from '../commands';
import {Network, WasmNetwork, ProxyNetwork} from '../network';

declare var CONFIG: any;
declare var remark: any;

export function start() {
  let network = CONFIG.wasm ? new WasmNetwork() : new ProxyNetwork();

  let md = null;
  network.onNativeMessage = function (msg) {
    console.log(msg);

    if (!md && msg.MarkdownUpdate) {
      md = msg.MarkdownUpdate;

      // Start the remark.js presentation.
      remark.create({
        source: md,
      });

      // Adds fullscreen button after remark is instantiated.
      let fullscreen = document.createElement('button');
      fullscreen.innerText = '↕️';
      fullscreen.onclick = function (e) {
        console.log('fullscreen attempt');
        let a = document.querySelector('.remark-slides-area');
        try {
          (a as any).mozRequestFullScreen();
        } catch (e) {
          (a as any).requestFullscreen();
        }
      };
      fullscreen.style.cssText = `
        position: fixed;
        top: 10px;
        left: 10px;
        z-index: 1000;
      `;
      document.body.appendChild(fullscreen);
    }
  }

  // Connect to remote sockets.
  network.nativeConnect()
  .then(() => network.syncConnect())
  .then(() => {
    console.log('edit-text initialized.');

    // Request markdown source immediately.
    let id = setInterval(function () {
      if (md !== null) {
        clearInterval(id);
      } else {
        try {
          network.nativeCommand(commands.RequestMarkdown());
        } catch (e) {
          // Socket may not be ready yet
        }
      }
    }, 250);
  });

};