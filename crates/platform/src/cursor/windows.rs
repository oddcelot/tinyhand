//! Windows: swap every system cursor for a blank one; `SPI_SETCURSORS` restores the user's.

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateCursor, OCR_APPSTARTING, OCR_CROSS, OCR_HAND, OCR_HELP, OCR_IBEAM, OCR_NO, OCR_NORMAL, OCR_SIZEALL,
    OCR_SIZENESW, OCR_SIZENS, OCR_SIZENWSE, OCR_SIZEWE, OCR_UP, OCR_WAIT, SPI_SETCURSORS, SetSystemCursor,
    SystemParametersInfoW,
};

/// `OCR_*` ids, plus the pin (32671) and person (32672) cursors of newer Windows.
const IDS: [u32; 16] = [
    OCR_NORMAL, OCR_IBEAM, OCR_WAIT, OCR_CROSS, OCR_UP, OCR_SIZENWSE, OCR_SIZENESW, OCR_SIZEWE, OCR_SIZENS,
    OCR_SIZEALL, OCR_NO, OCR_HAND, OCR_APPSTARTING, OCR_HELP, 32671, 32672,
];

#[derive(Default)]
pub struct Os {
    swapped: bool,
}

impl Os {
    pub fn hide(&mut self) {
        // A 32×32 monochrome cursor: AND mask all ones, XOR mask all zeros = transparent.
        let and = [0xFFu8; 128];
        let xor = [0u8; 128];
        // SAFETY: SetSystemCursor takes ownership of (and destroys) each cursor we make, so
        // every id gets its own. Masks outlive the calls.
        unsafe {
            let module = GetModuleHandleW(std::ptr::null());
            for id in IDS {
                let blank = CreateCursor(module, 0, 0, 32, 32, and.as_ptr().cast(), xor.as_ptr().cast());
                if !blank.is_null() {
                    SetSystemCursor(blank, id);
                }
            }
        }
        self.swapped = true;
    }

    /// Restores the user's cursors, however many times `hide` ran.
    pub fn show(&mut self) {
        if self.swapped {
            // SAFETY: reloads the system cursors from the registry.
            unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, std::ptr::null_mut(), 0) };
            self.swapped = false;
        }
    }

    pub fn seed(&self) -> i32 {
        0
    }

    /// A swapped system cursor stays swapped; nothing to do.
    pub fn reassert(&mut self) {}
}

impl Drop for Os {
    fn drop(&mut self) {
        self.show();
    }
}

pub fn enable_background_hiding() {}
