export const config: RTCConfiguration = {
    iceServers: [
        {
            urls: [
                'stun.l.google.com:19302',
                'stun1.l.google.com:19302',
                'stun2.l.google.com:19302',
                'stun3.l.google.com:19302',
                'stun4.l.google.com:19302',
            ],
        },
    ],
};
