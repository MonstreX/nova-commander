use super::*;

fn file_with(bytes: &[u8]) -> (tempfile::TempDir, InStream) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("data.bin");
    std::fs::write(&path, bytes).unwrap();
    let stream = InStream::file(&path).unwrap();
    (folder, stream)
}

/// A file read from where it stands, moved from its start, where it is and its end; past the end reads
/// nothing; before the start is 7-Zip's negative seek.
#[test]
fn a_file_is_read_and_moved_in() {
    let (_folder, stream) = file_with(b"0123456789");
    let mut buffer = [0u8; 4];
    assert_eq!(stream.read(&mut buffer).unwrap(), 4);
    assert_eq!(&buffer, b"0123");
    assert_eq!(stream.seek(SeekFrom::Current(2)).unwrap(), 6);
    assert_eq!(stream.read(&mut buffer).unwrap(), 4);
    assert_eq!(&buffer, b"6789");
    assert_eq!(stream.seek(SeekFrom::End(-3)).unwrap(), 7);
    assert_eq!(stream.read(&mut buffer).unwrap(), 3);
    assert_eq!(stream.seek(SeekFrom::Start(20)).unwrap(), 20);
    assert_eq!(stream.read(&mut buffer).unwrap(), 0);
    let error = stream.seek(SeekFrom::End(-11)).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(NEGATIVE_SEEK));
}

/// A stream from an offset on: its start is there, its end the file's; its places count from its start.
#[test]
fn a_stream_from_an_offset_starts_there() {
    let (_folder, stream) = file_with(b"stubPK-archive");
    let sub = stream.from_offset(4).unwrap();
    let mut buffer = [0u8; 2];
    assert_eq!(sub.read(&mut buffer).unwrap(), 2);
    assert_eq!(&buffer, b"PK");
    assert_eq!(sub.seek(SeekFrom::Current(0)).unwrap(), 2);
    assert_eq!(sub.seek(SeekFrom::Start(3)).unwrap(), 3);
    assert_eq!(sub.read(&mut buffer).unwrap(), 2);
    assert_eq!(&buffer, b"ar");
    assert_eq!(sub.seek(SeekFrom::End(0)).unwrap(), 10);
    assert!(sub.seek(SeekFrom::Current(-100)).is_err());
}

/// The object answers for `IUnknown`, `ISequentialInStream` and `IInStream`, nothing else; each answer
/// is a reference of its own.
#[test]
fn the_object_has_the_stream_interfaces() {
    let (_folder, stream) = file_with(b"x");
    for iid in [IID_UNKNOWN, IID_SEQUENTIAL_IN_STREAM, IID_IN_STREAM] {
        let other = stream.0.query(&iid).unwrap();
        assert_eq!(other.raw(), stream.0.raw());
    }
    assert!(stream.0.query(&super::super::com::IID_IN_ARCHIVE).is_none());
    let copy = stream.clone();
    drop(stream);
    let mut buffer = [0u8; 1];
    assert_eq!(copy.read(&mut buffer).unwrap(), 1);
}

/// A system error code is `HRESULT_FROM_WIN32`'s; an `HRESULT` stays.
#[test]
fn errors_are_hresults() {
    assert_eq!(hresult_of(5), 0x8007_0005_u32.cast_signed());
    assert_eq!(hresult_of(0x8_2345), 0x8007_2345_u32.cast_signed());
    assert_eq!(hresult_of(E_FAIL), E_FAIL);
    assert_eq!(hresult_of(0), 0);
}

/// A place before a file's start from where it stands is 7-Zip's negative seek; an origin not known
/// fails, for a file and for a stream from an offset.
#[test]
fn a_seek_before_the_start_or_from_nowhere_fails() {
    let (folder, stream) = file_with(b"0123456789");
    stream.seek(SeekFrom::Start(3)).unwrap();
    let error = stream.seek(SeekFrom::Current(-4)).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(NEGATIVE_SEEK));
    let file = File::open(folder.path().join("data.bin")).unwrap();
    let mut backing = Backing::File {
        file,
        size: 10,
        position: 0,
    };
    assert_eq!(backing.seek(0, 3), Err(E_FAIL));
    let mut sub = Backing::Sub {
        base: stream.clone(),
        offset: 2,
    };
    assert_eq!(sub.seek(0, 3), Err(E_FAIL));
    assert_eq!(sub.seek(1, SEEK_SET), Ok(1));
    assert_eq!(stream.seek(SeekFrom::Current(0)).unwrap(), 3, "the base's");
}

/// A stream from an offset of one from an offset: the offsets add up.
#[test]
fn a_stream_from_an_offset_of_another_starts_further() {
    let (_folder, stream) = file_with(b"0123456789");
    let inner = stream.from_offset(2).unwrap().from_offset(3).unwrap();
    let mut buffer = [0u8; 2];
    assert_eq!(inner.read(&mut buffer).unwrap(), 2);
    assert_eq!(&buffer, b"56");
    assert_eq!(inner.seek(SeekFrom::Start(0)).unwrap(), 0);
    assert_eq!(inner.read(&mut buffer).unwrap(), 2);
    assert_eq!(&buffer, b"56");
    assert_eq!(inner.seek(SeekFrom::End(0)).unwrap(), 5);
}

/// The library's reads and seeks: nothing asked reads nothing (with no buffer too), a buffer missing
/// fails, the count's or the place's answer may be missing.
#[test]
fn the_librarys_reads_are_answered() {
    let (_folder, stream) = file_with(b"abc");
    let raw = stream.0.raw();
    let mut done = 9u32;
    let mut byte = 0u8;
    // SAFETY: the stream's own functions on it, with places of the test's.
    unsafe {
        assert_eq!(read(raw, (&raw mut byte).cast(), 0, &raw mut done), S_OK);
        assert_eq!(done, 0);
        assert_eq!(read(raw, std::ptr::null_mut(), 0, &raw mut done), S_OK);
        done = 9;
        assert_eq!(read(raw, std::ptr::null_mut(), 1, &raw mut done), E_FAIL);
        assert_eq!(done, 0);
        assert_eq!(
            read(raw, (&raw mut byte).cast(), 1, std::ptr::null_mut()),
            S_OK
        );
        assert_eq!(byte, b'a');
        let mut at = 0u64;
        assert_eq!(seek(raw, 1, SEEK_CUR, &raw mut at), S_OK);
        assert_eq!(at, 2);
        assert_eq!(seek(raw, 0, SEEK_SET, std::ptr::null_mut()), S_OK);
    }
}

/// Each reference counted; an interface asked with no place for it fails, one not had is none.
#[test]
fn the_librarys_references_and_questions_are_answered() {
    let (_folder, stream) = file_with(b"abc");
    let raw = stream.0.raw();
    // SAFETY: the stream's own functions on it, with places of the test's.
    unsafe {
        assert_eq!(add_ref(raw), 2);
        assert_eq!(release(raw), 1);
        assert_eq!(query(raw, &IID_IN_STREAM, std::ptr::null_mut()), E_FAIL);
        let mut out: Raw = raw;
        assert_eq!(query(raw, std::ptr::null(), &raw mut out), E_FAIL);
        let other = super::super::com::IID_IN_ARCHIVE;
        assert_eq!(query(raw, &other, &raw mut out), E_NOINTERFACE);
        assert!(out.is_null());
    }
}

/// A missing file is not opened.
#[test]
fn a_missing_file_is_an_error() {
    let folder = tempfile::tempdir().unwrap();
    assert!(InStream::file(&folder.path().join("none")).is_err());
}
