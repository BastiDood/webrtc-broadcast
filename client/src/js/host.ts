import { config } from './peer';

async function main() {
    const video = document.getElementById('screen');
    const isVideo = video instanceof HTMLVideoElement;
    if (!isVideo)
        throw new Error('no video found');

    const ws = new WebSocket(process.env.WS_HOST!);
    await new Promise(resolve => ws.addEventListener('open', resolve, {
        passive: true,
        once: true,
    }));

    const peer = new RTCPeerConnection(config);
    console.log(await peer.createOffer());
    peer.addEventListener('negotiationneeded', async function() {
        // Signal offer to the remote peer
        const offer = await this.createOffer();
        await this.setLocalDescription(offer);
        ws.send(JSON.stringify(offer));
        console.log(offer);

        // Extract the first message as the answer
        const answer: RTCSessionDescriptionInit = await new Promise(resolve => ws.addEventListener(
            'message',
            ({ data }) => resolve(JSON.parse(data)),
            { passive: true, once: true, },
        ));
        await peer.setRemoteDescription(answer);
        console.log('Received answer...');

        ws.addEventListener('message', ({ data }) => {
            if (typeof data !== 'string')
                throw new Error('non-string ICE candidate');
            const json = JSON.parse(data);
            return peer.addIceCandidate(json);
        }, { passive: true });
    }, { passive: true, once: true });

    // Request camera so that negotiation begins
    const media = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
    video.srcObject = media;
    for (const track of media.getVideoTracks())
        peer.addTrack(track);
}

main();
