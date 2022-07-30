import { config } from './peer';

async function main() {
    const video = document.getElementById('screen');
    const isVideo = video instanceof HTMLVideoElement;
    if (!isVideo) throw new Error('no video found');

    // Attempt to connect first and block until open
    const ws = new WebSocket(process.env.WS_HOST!);
    await new Promise(resolve => ws.addEventListener('open', resolve, {
        passive: true,
        once: true,
    }));

    // Wait for remote peer's offer
    const offer: RTCSessionDescriptionInit = await new Promise(resolve => ws.addEventListener(
        'message',
        ({ data }) => resolve(JSON.parse(data)),
        { passive: true, once: true, },
    ));

    // Respond to the remote with an answer
    const peer = new RTCPeerConnection(config);

    peer.addEventListener('icecandidate', ({ candidate }) => {
        if (candidate === null)
            return;
        const json = candidate.toJSON();
        ws.send(JSON.stringify(json));
    }, { passive: true });

    peer.addEventListener('track', ({ streams: [stream] }) => {
        if (stream)
            video.srcObject = stream;
    }, { passive: true, once: true });

    ws.addEventListener('message', ({ data }) => {
        if (typeof data !== 'string')
            throw new Error('non-string ICE candidate');
        const json = JSON.parse(data);
        return peer.addIceCandidate(json);
    }, { passive: true });

    await peer.setRemoteDescription(offer);
    const answer = await peer.createAnswer();
    await peer.setLocalDescription(answer);
    ws.send(JSON.stringify(answer));
}

main();
