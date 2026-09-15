use anyhow::{Result, bail};
use ledalert::{config::MAX_LEDS, wled::visit_ddp_packets};

#[test]
fn complete_frames_round_trip_across_mtu_boundaries() -> Result<()> {
    for count in [1, 480, 481, MAX_LEDS] {
        let pixels: Vec<[u8; 3]> = (0..count)
            .map(|index| {
                [
                    index as u8,
                    (index >> 8) as u8,
                    (index.wrapping_mul(37)) as u8,
                ]
            })
            .collect();
        let mut reconstructed = Vec::new();
        let mut packets = 0;
        visit_ddp_packets(&pixels, 9, |packet| {
            let offset = u32::from_be_bytes(packet[4..8].try_into()?) as usize;
            let length = u16::from_be_bytes(packet[8..10].try_into()?) as usize;
            assert_eq!(
                offset,
                reconstructed.len(),
                "offsets are RGB byte offsets, not LED indexes"
            );
            assert_eq!(length, packet.len() - 10);
            assert!(length <= 1440 && length > 0 && length.is_multiple_of(3));
            assert!(packet.len() <= 1450);
            assert_eq!(&packet[1..4], &[9, 0x0b, 1]);
            assert_eq!(
                packet[0],
                if offset + length == count * 3 {
                    0x41
                } else {
                    0x40
                }
            );
            reconstructed.extend_from_slice(&packet[10..]);
            packets += 1;
            Ok(())
        })?;
        assert_eq!(packets, count.div_ceil(480));
        assert_eq!(reconstructed, pixels.as_flattened());
    }
    Ok(())
}

#[test]
fn sequence_is_preserved_on_every_datagram_and_invalid_frames_never_visit() -> Result<()> {
    let pixels = [[1, 2, 3]; 481];
    for sequence in 1..=15 {
        visit_ddp_packets(&pixels, sequence, |packet| {
            assert_eq!(packet[1], sequence);
            Ok(())
        })?;
    }
    for sequence in [0, 16, 255] {
        assert!(
            visit_ddp_packets(&pixels, sequence, |_| panic!(
                "invalid sequence reached sender"
            ))
            .is_err()
        );
    }
    assert!(visit_ddp_packets(&[], 1, |_| panic!("empty frame reached sender")).is_err());
    assert!(
        visit_ddp_packets(&vec![[0; 3]; MAX_LEDS + 1], 1, |_| panic!(
            "oversized frame reached sender"
        ))
        .is_err()
    );
    Ok(())
}

#[test]
fn a_failed_datagram_does_not_continue_to_push_an_incomplete_frame() {
    let mut packets = 0;
    let result = visit_ddp_packets(&[[4, 5, 6]; 961], 1, |_| {
        packets += 1;
        bail!("synthetic UDP failure")
    });
    assert!(result.is_err());
    assert_eq!(
        packets, 1,
        "later datagrams, including PUSH, must not be emitted after failure"
    );
}
