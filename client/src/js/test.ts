export async function main() {
    const video = document.getElementById('screen');
    const isVideo = video instanceof HTMLVideoElement;
    if (!isVideo)
        throw new Error('no video found');

    const media = await navigator.mediaDevices.getUserMedia({
        video: true,
        audio: false,
    });
    console.log('Video feed connected...');

    const p1 = new RTCPeerConnection();
    const p2 = new RTCPeerConnection();

    p1.addEventListener('negotiationneeded', async function() {
        console.log('Negotiation needed...')

        const offer = await this.createOffer();
        await this.setLocalDescription(offer);
        await p2.setRemoteDescription(offer);
        console.log('Offers exchanged...');

        const answer = await p2.createAnswer();
        await p2.setLocalDescription(answer);
        await this.setRemoteDescription(answer);
        console.log('Answers exchanged...');
    }, { passive: true });

    p2.addEventListener('track', ({ streams: [stream] }) => {
        console.log('On track for p2 invoked...');
        console.log(stream);
        if (stream)
            video.srcObject = stream;
    }, { passive: true });

    p1.addEventListener('icecandidate', ({ candidate }) => {
        if (candidate === null)
            return;
        p2.addIceCandidate(candidate);
        console.log('p1 has a new candidate...');
    }, { passive: true });

    p2.addEventListener('icecandidate', ({ candidate }) => {
        if (candidate === null)
            return;
        p1.addIceCandidate(candidate);
        console.log('p2 has a new candidate...');
    }, { passive: true });

    for (const track of media.getVideoTracks()) {
        p1.addTrack(track, media);
        console.log('Track added...');
    }
}

main().catch(console.error);
