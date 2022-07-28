import { config } from './peer';

const peer = new RTCPeerConnection(config);
const offer = await peer.createOffer();
await peer.setLocalDescription(offer);

const ws = new WebSocket('/ws/client', JSON.stringify(offer));

ws.addEventListener('open', function() {
    peer.addEventListener('icecandidate', ({ candidate }) => {
        if (candidate === null) throw new Error('null candidate');
        const json = candidate.toJSON();
        ws.send(JSON.stringify(json));
    }, { passive: true });

    let hasAnswer = false;
    this.addEventListener('message', async ({ data }) => {
        if (typeof data !== 'string') throw new Error('non-string ICE candidate');

        const init = JSON.parse(data);
        if (hasAnswer) {
            await peer.addIceCandidate(init);
            return;
        }

        // Finish the handshake
        await peer.setRemoteDescription(init);
        hasAnswer = true;

        // Only request camera permissions when handshake is done
        const media = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
        for (const track of media.getVideoTracks()) peer.addTrack(track);
    }, { passive: true });
}, { passive: true, once: true });
