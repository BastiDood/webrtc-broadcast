import { config } from './peer';

const peer = new RTCPeerConnection(config);
const offer = await peer.createOffer({
    offerToReceiveVideo: true,
    offerToReceiveAudio: false,
});
await peer.setLocalDescription(offer);

const response = await fetch('/api/client', {
    method: 'POST',
    body: JSON.stringify(offer),
});

if (!response.ok) throw new Error('no host available yet');

interface HostFound {
    /** Answer from the host stream. */
    sdp: RTCSessionDescriptionInit;
    /** Client-specific code used for identification. */
    code: string;
}

const { sdp, code }: HostFound = await response.json();
const answer = new RTCSessionDescription(sdp);
await peer.setRemoteDescription(answer);

const ws = new WebSocket('/ws/client?code=' + code);
peer.addEventListener('icecandidate', ({ candidate }) => {
    if (candidate === null) throw new Error('null candidate');
    const json = candidate.toJSON();
    ws.send(JSON.stringify(json));
}, { passive: true });
ws.addEventListener('message', async ({ data }) => {
    if (typeof data !== 'string') throw new Error('non-string ICE candidate');
    const init: RTCIceCandidateInit = JSON.parse(data);
    await peer.addIceCandidate(init);
}, { passive: true });
