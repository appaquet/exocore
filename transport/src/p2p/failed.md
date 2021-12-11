SENT 11 BYTES TO STREAM
11 12:19:45.417 TRACE exocore_transport::p2p::protoc - Successfully sent message. Substream was consumed (had streaming).
11 12:19:45.417 TRACE yamux::frame::io               - 3a205857: read: (ReadState::Header 0)
11 12:19:45.418 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.418 TRACE yamux::connection              - 3a205857: sending: (Header Data 4 (len 4) (flags 0))
11 12:19:45.418 TRACE libp2p_noise::io               - write: buffered 12 bytes
11 12:19:45.418 TRACE libp2p_noise::io               - write: buffered 16 bytes
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.418 TRACE libp2p_noise::io               - flush: sending 16 bytes
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write: cipher text len = 32 bytes
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write state WriteLen { len: 32, buf: [0, 32], off: 0 }
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write: frame len (32, [0, 32], 0/2)
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write state WriteData { len: 32, off: 0 }
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write: 32/32 bytes written
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write: finished with 32 bytes
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.418 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 0)
11 12:19:45.418 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.418 TRACE libp2p_noise::io::framed       - read: frame len = 32
11 12:19:45.418 TRACE libp2p_noise::io::framed       - read state: ReadData { len: 32, off: 0 }
11 12:19:45.418 TRACE yamux::connection              - 3a205857: removing dropped (Stream 3a205857/4)
11 12:19:45.418 TRACE yamux::connection::stream      - 3a205857/4: update state: (Open Closed Closed)
11 12:19:45.418 TRACE yamux::connection              - 3a205857: sending: (Header Data 4 (len 0) (flags 8))
11 12:19:45.418 TRACE libp2p_noise::io               - write: buffered 12 bytes
11 12:19:45.418 TRACE libp2p_noise::io::framed       - read: 32/32 bytes
11 12:19:45.418 TRACE libp2p_noise::io::framed       - read: decrypting 32 bytes
11 12:19:45.418 TRACE yamux::frame::io               - 3a205857: read: (ReadState::Header 0)
11 12:19:45.418 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.418 TRACE yamux::connection              - 3a205857: sending: (Header Data 4 (len 200) (flags 0))
11 12:19:45.418 TRACE libp2p_noise::io               - write: buffered 24 bytes
11 12:19:45.418 TRACE libp2p_noise::io               - write: buffered 224 bytes
11 12:19:45.418 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.418 TRACE libp2p_noise::io::framed       - read: payload len = 16 bytes
11 12:19:45.418 TRACE libp2p_noise::io               - flush: sending 224 bytes
11 12:19:45.419 TRACE libp2p_noise::io               - read: copied 12/16 bytes
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 12)
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (Header Data 4 (len 4) (flags 0))
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Body (header (Header Data 4 (len 4) (flags 0))) (offset 0) (buffer-len 4))
11 12:19:45.419 TRACE libp2p_noise::io               - read: copied 16/16 bytes
11 12:19:45.419 TRACE libp2p_noise::io               - read: frame consumed
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Body (header (Header Data 4 (len 4) (flags 0))) (offset 4) (buffer-len 4))
11 12:19:45.419 TRACE yamux::connection              - c40df57e: received: (Header Data 4 (len 4) (flags 0))
11 12:19:45.419 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Init)
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 0)
11 12:19:45.419 TRACE libp2p_noise::io::framed       - read state: Ready
11 12:19:45.419 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.419 TRACE libp2p_noise::io::framed       - write: cipher text len = 240 bytes
11 12:19:45.419 TRACE yamux::connection::stream      - c40df57e/4: read 4 bytes
11 12:19:45.419 TRACE libp2p_noise::io::framed       - write state WriteLen { len: 240, buf: [0, 240], off: 0 }
11 12:19:45.419 TRACE libp2p_noise::io::framed       - write: frame len (240, [0, 240], 0/2)
11 12:19:45.419 TRACE libp2p_noise::io::framed       - write state WriteData { len: 240, off: 0 }
11 12:19:45.419 TRACE libp2p_noise::io::framed       - write: 240/240 bytes written
11 12:19:45.419 TRACE libp2p_noise::io::framed       - write: finished with 240 bytes
11 12:19:45.419 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 0)
11 12:19:45.419 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.419 TRACE libp2p_noise::io::framed       - read: frame len = 240
11 12:19:45.419 TRACE libp2p_noise::io::framed       - read state: ReadData { len: 240, off: 0 }
11 12:19:45.419 TRACE libp2p_noise::io::framed       - read: 240/240 bytes
11 12:19:45.419 TRACE libp2p_noise::io::framed       - read: decrypting 240 bytes
11 12:19:45.419 TRACE libp2p_noise::io::framed       - read: payload len = 224 bytes
11 12:19:45.419 TRACE libp2p_noise::io               - read: copied 12/224 bytes
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 12)
11 12:19:45.419 TRACE yamux::frame::io               - c40df57e: read: (Header Data 4 (len 0) (flags 8))
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Body (header (Header Data 4 (len 0) (flags 8))) (offset 0) (buffer-len 0))
11 12:19:45.420 TRACE yamux::frame::io               - 3a205857: read: (ReadState::Header 0)
11 12:19:45.420 TRACE yamux::connection              - c40df57e: received: (Header Data 4 (len 0) (flags 8))
11 12:19:45.420 TRACE yamux::connection::stream      - c40df57e/4: update state: (Open Closed Closed)
11 12:19:45.420 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.420 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.420 TRACE yamux::connection              - 3a205857: sending: (Header Data 4 (len 11) (flags 0))
11 12:19:45.420 TRACE libp2p_noise::io               - write: buffered 12 bytes
11 12:19:45.420 TRACE libp2p_noise::io               - write: buffered 23 bytes
11 12:19:45.420 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.420 TRACE libp2p_noise::io               - flush: sending 23 bytes
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Init)
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 0)
11 12:19:45.420 TRACE libp2p_noise::io               - read: copied 24/224 bytes
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 12)
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (Header Data 4 (len 200) (flags 0))
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Body (header (Header Data 4 (len 200) (flags 0))) (offset 0) (buffer-len 200))
11 12:19:45.420 TRACE libp2p_noise::io               - read: copied 224/224 bytes
11 12:19:45.420 TRACE libp2p_noise::io               - read: frame consumed
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Body (header (Header Data 4 (len 200) (flags 0))) (offset 200) (buffer-len 200))
11 12:19:45.420 TRACE yamux::connection              - c40df57e: received: (Header Data 4 (len 200) (flags 0))
11 12:19:45.420 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Init)
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 0)
11 12:19:45.420 TRACE libp2p_noise::io::framed       - read state: Ready
11 12:19:45.420 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.420 TRACE yamux::connection::stream      - c40df57e/4: read 200 bytes
WITH STREAM msg_len: 200
11 12:19:45.420 TRACE exocore_transport::p2p::protoc - Successfully received message, with stream.
11 12:19:45.420 TRACE libp2p_noise::io::framed       - write: cipher text len = 39 bytes
11 12:19:45.420 TRACE libp2p_noise::io::framed       - write state WriteLen { len: 39, buf: [0, 39], off: 0 }
11 12:19:45.420 TRACE libp2p_noise::io::framed       - write: frame len (39, [0, 39], 0/2)
11 12:19:45.420 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 0)
11 12:19:45.421 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.421 TRACE libp2p_noise::io::framed       - read: frame len = 39
11 12:19:45.421 TRACE libp2p_noise::io::framed       - read state: ReadData { len: 39, off: 0 }
11 12:19:45.421 TRACE libp2p_noise::io::framed       - write state WriteData { len: 39, off: 0 }
11 12:19:45.421 TRACE libp2p_noise::io::framed       - write: 39/39 bytes written
11 12:19:45.421 TRACE exocore_transport::p2p::behavi - Received message from Node{moderately-smashing-hornet}
11 12:19:45.421 TRACE exocore_transport::p2p::transp - Got message from 12D3KooWEcRU4VNgVEGSjfmKkXmMb3jbhdAWFwcpogmB6ca8oge6
11 12:19:45.421 TRACE yamux::frame::io               - c40df57e: read: (ReadState::Header 0)
11 12:19:45.421 TRACE libp2p_noise::io::framed       - read state: ReadData { len: 39, off: 0 }
11 12:19:45.421 TRACE libp2p_noise::io::framed       - write: finished with 39 bytes
11 12:19:45.421 TRACE libp2p_noise::io::framed       - write state Ready
11 12:19:45.421 TRACE yamux::frame::io               - 3a205857: read: (ReadState::Header 0)
11 12:19:45.421 TRACE libp2p_noise::io::framed       - read state: ReadLen { buf: [0, 0], off: 0 }
11 12:19:45.421 DEBUG yamux::connection::stream      - c40df57e/4: eof
11 12:19:45.421 TRACE libp2p_noise::io::framed       - read: 39/39 bytes
11 12:19:45.421 TRACE libp2p_noise::io::framed       - read: decrypting 39 bytes