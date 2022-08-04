import { config } from './peer';

async function main() {

    const select = document.getElementById('input-selection');
    const isSelect = select instanceof HTMLSelectElement;
    if (!isSelect)
        throw new Error('input selector not found');

    const devices = await navigator.mediaDevices.enumerateDevices();
    for (const dev of devices) {
        if (dev.kind !== 'videoinput')
            continue;

        const option = document.createElement('option');
        option.innerText = dev.label || 'Unknown';
        option.value = dev.deviceId;
        select.appendChild(option);
    }

    // Wait for user to select item
    const { target } = await new Promise<Event>(resolve => select.addEventListener('change', resolve, {
        passive: true,
        once: true,
    }));
    const isOption = target instanceof HTMLSelectElement;
    if (!isOption)
        throw new Error('<option> element not found');

    const ws = new WebSocket(process.env.WS_HOST!);
    await new Promise(resolve => ws.addEventListener('open', resolve, {
        passive: true,
        once: true,
    }));

    const peer = new RTCPeerConnection(config);

    peer.addEventListener('negotiationneeded', async function() {
        // Signal offer to the remote peer
        const offer = await this.createOffer();
        ws.send(JSON.stringify(offer));

        // Extract the first message as the answer
        const answer: RTCSessionDescriptionInit = await new Promise(resolve => ws.addEventListener(
            'message',
            ({ data }) => resolve(JSON.parse(data)),
            { passive: true, once: true, },
        ));
        await this.setLocalDescription(offer);
        await peer.setRemoteDescription(answer);

        // Then keep receiving new ice candidates
        ws.addEventListener('message', ({ data }) => {
            if (typeof data !== 'string')
                throw new Error('non-string ICE candidate');
            const json = JSON.parse(data);
            return peer.addIceCandidate(json);
        }, { passive: true });
    }, { passive: true, once: true });

    peer.addEventListener('icecandidate', ({ candidate }) => {
        if (candidate === null)
            return;
        const json = candidate.toJSON();
        ws.send(JSON.stringify(json));
    });

    // Request camera so that negotiation begins
    const media = await navigator.mediaDevices.getUserMedia({
        audio: false,
        video: { deviceId: { exact: target.value } },
    });
    for (const track of media.getVideoTracks())
        peer.addTrack(track, media);

    // Create <video> for self-view
    const video = document.createElement('video');
    video.playsInline = true;
    video.autoplay = true;
    video.muted = true;
    video.srcObject = media;
    select.replaceWith(video);
}

main().catch(console.error);
