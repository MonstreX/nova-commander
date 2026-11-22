use super::*;

/// The ids as 7-Zip's `IDecl.h` builds them: `IInArchive` is `{23170F69-40C1-278A-0000-000600600000}`.
#[test]
fn ids_are_7zips() {
    assert_eq!(
        id_number(&IID_IN_ARCHIVE),
        0x2317_0F69_40C1_278A_0000_0006_0060_0000
    );
    assert_eq!(
        id_number(&IID_IN_STREAM),
        0x2317_0F69_40C1_278A_0000_0003_0003_0000
    );
    assert_eq!(
        id_number(&IID_CRYPTO_GET_TEXT_PASSWORD),
        0x2317_0F69_40C1_278A_0000_0005_0010_0000
    );
    assert_eq!(
        id_number(&IID_OPEN_VOLUME_CALLBACK),
        0x2317_0F69_40C1_278A_0000_0006_0030_0000
    );
    assert_eq!(
        id_number(&IID_UNKNOWN),
        0x0000_0000_0000_0000_c000_0000_0000_0046
    );
    assert!(same(&IID_IN_STREAM, &IID_IN_STREAM));
    assert!(!same(&IID_IN_STREAM, &IID_SEQUENTIAL_IN_STREAM));
}

fn variant(vt: u16, value: u64) -> PropVariant {
    PropVariant {
        vt,
        reserved: [0; 3],
        value: [value, 0],
    }
}

/// The unsigned types 7-Zip gives read as their value, the narrow ones cut to their size.
#[test]
fn unsigned_values_are_read_by_their_type() {
    assert_eq!(variant(VT_BOOL, 0xFFFF).read(false), Prop::Bool(true));
    assert_eq!(variant(VT_BOOL, 0).read(false), Prop::Bool(false));
    assert_eq!(variant(VT_UI1, 0x1FF).read(false), Prop::U32(0xFF));
    assert_eq!(variant(VT_UI2, 0x1_FFFF).read(false), Prop::U32(0xFFFF));
    assert_eq!(variant(VT_UI4, 0x1_0000_0002).read(false), Prop::U32(2));
    assert_eq!(variant(VT_UINT, 7).read(false), Prop::U32(7));
    assert_eq!(variant(VT_UI8, u64::MAX).read(false), Prop::U64(u64::MAX));
}

/// The signed ones widened with their sign; a time, nothing and a type not read.
#[test]
fn signed_and_other_values_are_read_by_their_type() {
    assert_eq!(variant(VT_I1, 0xFF).read(false), Prop::I64(-1));
    assert_eq!(variant(VT_I2, 0xFFFE).read(false), Prop::I64(-2));
    assert_eq!(variant(VT_I4, 0xFFFF_FFFD).read(false), Prop::I64(-3));
    assert_eq!(variant(VT_INT, 5).read(false), Prop::I64(5));
    assert_eq!(variant(VT_I8, u64::MAX).read(false), Prop::I64(-1));
    assert_eq!(variant(VT_FILETIME, 42).read(false), Prop::Time(42));
    assert_eq!(variant(VT_EMPTY, 0).read(false), Prop::Empty);
    assert_eq!(variant(99, 0).read(false), Prop::Other(99));
}

/// A text and binary data in a `BSTR`, freed with the variant; a null one reads as nothing.
#[test]
fn bstrs_are_read_as_text_or_bytes() {
    let text = variant(VT_BSTR, new_bstr("héllo") as usize as u64);
    assert_eq!(text.read(false), Prop::Text("héllo".to_string()));
    let bytes = variant(VT_BSTR, new_bstr("AB") as usize as u64);
    assert_eq!(bytes.read(true), Prop::Bytes(vec![b'A', 0, b'B', 0]));
    assert_eq!(variant(VT_BSTR, 0).read(false), Prop::Text(String::new()));
    assert_eq!(take_bstr(std::ptr::null_mut()), None);
    assert_eq!(take_bstr(new_bstr("x")), Some("x".to_string()));
}

/// What is written for the library reads back the same; a value it has no type for is left empty;
/// a null place is not written.
#[test]
fn written_values_read_back() {
    let props = [
        Prop::Bool(true),
        Prop::Bool(false),
        Prop::U32(7),
        Prop::U64(1 << 40),
        Prop::I64(-5),
        Prop::Time(123),
        Prop::Text("name.7z".to_string()),
    ];
    for prop in props {
        let mut out = PropVariant::empty();
        // SAFETY: a live variant of the test's.
        unsafe { PropVariant::write(&raw mut out, &prop) };
        assert_eq!(out.read(false), prop);
    }
    let mut out = PropVariant::empty();
    // SAFETY: a live variant of the test's.
    unsafe { PropVariant::write(&raw mut out, &Prop::Bytes(vec![1])) };
    assert_eq!(out.read(false), Prop::Empty);
    // SAFETY: null is allowed and not written.
    unsafe { PropVariant::write(std::ptr::null_mut(), &Prop::U32(1)) };
}

/// The other ids as `IDecl.h` builds them.
#[test]
fn the_other_ids_are_7zips() {
    let ids = [
        (IID_SEQUENTIAL_IN_STREAM, 0x0003_0001_0000),
        (IID_ALLOW_TAIL, 0x0006_0005_0000),
        (IID_OPEN_CALLBACK, 0x0006_0010_0000),
        (IID_GET_STREAM, 0x0006_0040_0000),
    ];
    for (id, low) in ids {
        assert_eq!(
            id_number(&id),
            0x2317_0F69_40C1_278A_0000_0000_0000_0000 | low
        );
    }
}

/// Each value written with the type the library reads it by: a true one as `VARIANT_TRUE` (all bits),
/// the rest of the value cleared.
#[test]
fn values_are_written_with_their_types() {
    let cases = [
        (Prop::Bool(true), VT_BOOL, 0xFFFF),
        (Prop::Bool(false), VT_BOOL, 0),
        (Prop::U32(7), VT_UI4, 7),
        (Prop::U64(1 << 40), VT_UI8, 1 << 40),
        (Prop::I64(-1), VT_I8, u64::MAX),
        (Prop::Time(9), VT_FILETIME, 9),
        (Prop::Other(3), VT_EMPTY, 0),
    ];
    for (prop, vt, value) in cases {
        let mut out = variant(VT_EMPTY, 0);
        out.value[1] = 0xDEAD;
        // SAFETY: a live variant of the test's.
        unsafe { PropVariant::write(&raw mut out, &prop) };
        assert_eq!((out.vt, out.value), (vt, [value, 0]), "{prop:?}");
    }
    let mut text = PropVariant::empty();
    // SAFETY: a live variant of the test's.
    unsafe { PropVariant::write(&raw mut text, &Prop::Text(String::new())) };
    assert_eq!(text.vt, VT_BSTR);
    assert_ne!(text.value[0], 0, "a BSTR even for no text");
}

/// A `VARIANT_BOOL` is 16 bits: what is above them is not read.
#[test]
fn a_bools_high_bits_are_not_read() {
    assert_eq!(variant(VT_BOOL, 0x1_0000).read(false), Prop::Bool(false));
    assert_eq!(variant(VT_BOOL, 1).read(false), Prop::Bool(true));
    assert_eq!(variant(VT_I1, 0x17F).read(false), Prop::I64(127));
    assert_eq!(variant(VT_I2, 0x1_7FFF).read(false), Prop::I64(0x7FFF));
}

/// A failed result is an error, a success (`S_FALSE` too) is not.
#[test]
fn results_are_checked() {
    assert!(check(S_OK).is_ok());
    assert!(check(S_FALSE).is_ok());
    assert!(check(E_FAIL).is_err());
    assert_eq!(check(E_ABORT).unwrap_err().raw_os_error(), Some(E_ABORT));
    assert!(check(INVALID_DATA).is_err() && check(E_NOINTERFACE).is_err());
    assert_eq!(Com::take(std::ptr::null_mut()).map(|com| com.raw()), None);
}
