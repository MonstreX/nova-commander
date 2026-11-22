//! The ABI's fingerprint (R7 review A3): the size and alignment of every struct it carries, folded into one
//! number that both tables carry ([`NcHost::abi`], [`NcPlugin::abi`]) and each side checks. A build whose
//! structs differ under the same API version is not loaded: the tables' sizes alone missed a change of
//! [`NcEntry`], [`NcKeyAnswer`] or [`NcDialogValue`]. A function's signature (a pointer either way) is not
//! seen: that change bumps [`API_VERSION`](crate::API_VERSION) by hand.

use core::mem::{align_of, size_of};

use crate::*;

/// FNV-1a over one more value.
const fn fold(hash: u64, value: usize) -> u64 {
    (hash ^ value as u64).wrapping_mul(0x0100_0000_01b3)
}

macro_rules! layouts {
    ($($kind:ty),* $(,)?) => {{
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        $(
            hash = fold(hash, size_of::<$kind>());
            hash = fold(hash, align_of::<$kind>());
        )*
        hash
    }};
}

/// The fingerprint of this build of the ABI's structs.
pub const ABI: u64 = layouts!(
    NcStr,
    NcHost,
    NcPluginInfo,
    NcPlugin,
    NcMenuItem,
    NcMenu,
    NcDialogItem,
    NcDialogEvent,
    NcDialogValue,
    NcDialogChange,
    NcDialog,
    NcEditorAction,
    NcEditorInfo,
    NcEditorAnswer,
    NcEntry,
    NcTimes,
    NcPanelInfo,
    NcInfoLine,
    NcPanelKey,
    NcKeyEvent,
    NcKeyAnswer,
    NcPanelMode,
    NcPanelState,
    NcPanelItem,
    NcScreen,
    NcPanelAction,
    NcGet,
    NcPut,
);
