# The Front-end
The UI uses vanilla JavaScript. Our chosen package manager is [pnpm]. This is not a strict requirement, though. Feel free to use other package managers (if necessary).

[pnpm]: https://pnpm.io

All HTML, CSS, and JS files are then bundled with [Parcel]. In accordance with default conventions, the bundled files are saved in the `dist/` folder.

Finally, one must set two required environment variables (via the shell or a `.env` file):

1. The `WS_HOST` variable determines the WebSocket URL that the host will use to connect to the signaling server. Any socket address with port `3000` will do fine as long as the path is set to `/ws/host`. For example, `ws://localhost:3000/ws/host` is a valid URL.
2. The `WS_CLIENT` variable determines the WebSocket URL that the clients will use to connect to the signaling server. It has the same restrictions as that of the `WS_HOST` variable except that the required path is now `/ws/client`. For example, `ws://localhost:3000/ws/client` is a valid URL.

[Parcel]: https://parceljs.org

In Bash, we run the following commands to build the static assets:

```bash
# Build the static assets to `dist/`
pnpm build

# Set up environment variables
export WS_HOST='ws://localhost:3000/ws/host'
export WS_CLIENT='ws://localhost:3000/ws/client'

# Host the files using any web server you want
# as long as we bind to any port but `3000`.
# Also, ensure that the server hosts `dist/`.
python -m http.server -d dist 1234
```
