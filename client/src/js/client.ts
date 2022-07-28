import { strict as assert } from 'assert';
import { config } from './peer';

const video = document.getElementById('screen');
assert(video instanceof HTMLVideoElement);

const peer = new RTCPeerConnection(config);
const offer = await peer.createOffer();
await peer.setLocalDescription(offer);

const ws = new WebSocket('/ws/host', JSON.stringify(offer));
ws.addEventListener('open', function() {
    let hasAnswer = false;
    this.addEventListener('message', async function({ data }) {
        if (typeof data !== 'string') throw new Error('non-string ICE candidate');

        const init = JSON.parse(data);
        if (hasAnswer) {
            await peer.addIceCandidate(init);
            return;
        }

        // Finish the handshake
        hasAnswer = true;
        await peer.setRemoteDescription(init);

        // Only start sending ice candidates from this point on
        peer.addEventListener('icecandidate', ({ candidate }) => {
            if (candidate === null) throw new Error('null candidate');
            const json = candidate.toJSON();
            this.send(JSON.stringify(json));
        }, { passive: true });

        // And then relay the track to the video
        peer.addEventListener('track', ({ streams }) => {
            assert(streams.length === 1);
            video.srcObject = streams[0];
        }, { passive: true, once: true });
    }, { passive: true });
}, { passive: true, once: true });
