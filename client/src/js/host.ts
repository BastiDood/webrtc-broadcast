import { config } from './peer';

// Register new stream
const response = await fetch('/api/host', { method: 'POST' });
switch (response.status) {
    case 201: break;
    case 401: throw new Error('host already exists');
    default: throw new Error('unknown status code');
}

const remotes: RTCPeerConnection[] = [];

interface NewClient {
    offer: RTCSessionDescriptionInit;
    code: string;
}

const code = await response.text();
const ws = new WebSocket('/ws/host?code=' + code);
ws.addEventListener('message', async ({ data }) => {
    if (typeof data !== 'string') return;

    const { offer, code }: NewClient = JSON.parse(data);
    const peer = new RTCPeerConnection(config);
    await peer.setRemoteDescription(offer);

    const answer = await peer.createAnswer();
    await peer.setLocalDescription(answer);
    ws.send(JSON.stringify({
        code,
        answer,
    }))

    peer.addEventListener('icecandidate', ({ candidate }) => {
        if (candidate === null) return;
        const json = candidate.toJSON();
        ws.send(JSON.stringify({
            code,
            ice: json,
        }));
    });

    remotes.push(peer);
});

