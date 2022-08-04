# WebRTC Broadcast
This project demonstrates an example implementation for a one-to-many broadcast stream via WebRTC. To be network-friendly towards the host, an intermediary server is involved so that the host uploads their stream only once (to the server). Then, the server clones and forwards the host's media packets to each of the client peers. Nevertheless, all of the media transport and stream maintenance are still handled by WebRTC.

See the READMEs of the [front-end](./client/README.md) and the [back-end](./signal/README.md) for more details on build instructions and overall architecture.
