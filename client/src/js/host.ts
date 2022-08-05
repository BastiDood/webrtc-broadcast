import { config } from './peer';

async function main() {
    const videoPrompt = document.getElementById('video-prompt');
    const isParagraph = videoPrompt instanceof HTMLParagraphElement;
    if (!isParagraph)
        throw new Error('<p> prompt not found');

    const devices = await navigator.mediaDevices.enumerateDevices();
    const select = document.createElement('select');
    select.title = 'Select Video Device';
    select.required = true;

    for (const { kind, label, deviceId } of devices)
        if (kind === 'videoinput') {
            const option = document.createElement('option');
            option.innerText = label || 'Unknown';
            option.value = deviceId;
            select.appendChild(option);
        }

    const submit = document.createElement('input');
    submit.type = 'submit';
    submit.value = 'Select Video Device';

    const form = document.createElement('form');
    form.appendChild(select).after(submit);
    videoPrompt.replaceWith(form);

    // Wait for user to select item
    const evt = await new Promise<SubmitEvent>(resolve => form.addEventListener('submit', resolve, { once: true }));
    evt.preventDefault();
    evt.stopPropagation();

    const video = document.createElement('video');
    video.playsInline = true;
    video.autoplay = true;
    video.muted = true;
    form.replaceWith(video);

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
        video: { deviceId: { exact: select.value } },
    });
    video.srcObject = media;

    // Trigger `negotiationneeded` event
    for (const track of media.getVideoTracks())
        peer.addTrack(track, media);
}

main().catch(console.error);
