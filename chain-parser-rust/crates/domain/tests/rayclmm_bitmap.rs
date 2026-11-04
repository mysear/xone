use xone_domain::{raydium_clmm::RaydiumCLMMBitmapExtension, Pool};
#[test]
fn bitmap_extension_index_width_and_profile_isolation() {
    let mut p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    let s = p.raydium_clmm.as_mut().unwrap();
    s.bitmap_extension = Some(RaydiumCLMMBitmapExtension {
        address: xone_domain::orca::PROGRAM_DATA.into(),
        positive: vec![0; 112],
        negative: vec![0; 112],
    });
    for index in [512, 1023, 1024, 7679, -513, -1024, -1025, -7680] {
        let e = s.bitmap_extension.as_mut().unwrap();
        let (words, offset) = if index >= 512 {
            (&mut e.positive, index - 512)
        } else {
            let group = (-index - 1) / 512 - 1;
            let bit = index + (group + 2) * 512;
            (&mut e.negative, group * 512 + bit)
        };
        words[offset as usize / 64] |= 1u64 << (offset % 64);
        assert!(s.array_initialized(index).unwrap());
    }
    assert!(p.validate().is_err());
    p.protocol = "raydium-clmm-bitmap".into();
    p.adapter_version = "raydium-clmm/6d7a0f82-spl-bitmap-1".into();
    assert!(p.validate().is_err());
    p.raydium_clmm
        .as_mut()
        .unwrap()
        .bitmap_extension
        .as_mut()
        .unwrap()
        .positive
        .pop();
    assert!(p.validate().is_err());
    p.raydium_clmm.as_mut().unwrap().bitmap_extension = None;
    assert!(p.validate().is_err());
}

#[test]
fn original_extension_index_vectors_match_source_flip_check() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-bitmap-indices.json"
    ))
    .unwrap();
    let vectors = f["vectors"].as_array().unwrap();
    assert!(vectors.len() >= 80);
    for v in vectors {
        let mut p: Pool =
            serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json"))
                .unwrap();
        let s = p.raydium_clmm.as_mut().unwrap();
        s.tick_spacing = 1;
        s.bitmap_extension = Some(RaydiumCLMMBitmapExtension {
            address: xone_domain::orca::PROGRAM_DATA.into(),
            positive: serde_json::from_value(v["positive"].clone()).unwrap(),
            negative: serde_json::from_value(v["negative"].clone()).unwrap(),
        });
        let index = v["index"].as_i64().unwrap() as i32;
        assert_eq!(
            s.array_initialized(index).unwrap(),
            v["expected_initialized"].as_bool().unwrap(),
            "{index}"
        );
        for neighbor in [index - 1, index + 1] {
            if (-7680..=7679).contains(&neighbor) && !(-512..=511).contains(&neighbor) {
                assert!(
                    !s.array_initialized(neighbor).unwrap(),
                    "spurious {index}/{neighbor}"
                );
            }
        }
    }
}
