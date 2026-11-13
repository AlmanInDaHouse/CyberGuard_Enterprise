//! The wizard's dialog (SPEC-024 Amendment 2026-11-13, points 4 to 7): a
//! Win32 dialog of `cg-setup`'s own, from a template built in memory, with
//! the user32 controls alone (no common controls library, so no manifest and
//! nothing the loader takes outside the system directory). It is topmost,
//! shown normally, asks for the foreground and flashes its taskbar button when
//! Windows refuses it. Its fields are the bootstrap file's keys, none
//! pre-filled; the token's is a single-line masked field, from which nothing
//! can be copied or cut and whose text another process cannot read, as
//! Windows' masked fields have it, and into which pasting is allowed. Each
//! submission is checked by the verb's `take`: a refusal of a field (24) or of
//! the anchor (25) shows in the dialog, which asks again with the fields as
//! they were; any other closes it. The dialog closes on a cancellation, 15
//! minutes after it showed with no accepted submission, or the exit of the
//! process that started the setup; its token field is cleared before it
//! closes.

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateDialogIndirectParamW, DestroyWindow, DispatchMessageW, FlashWindowEx, GetDlgItem,
    GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsDialogMessageW,
    KillTimer, PostThreadMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    SetWindowTextW, ShowWindow, TranslateMessage, BN_CLICKED, BS_DEFPUSHBUTTON, BS_PUSHBUTTON,
    DS_CENTER, DS_MODALFRAME, DS_SETFONT, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE,
    ES_PASSWORD, ES_WANTRETURN, FLASHWINFO, FLASHW_ALL, FLASHW_TIMERNOFG, GWLP_USERDATA, IDCANCEL,
    IDOK, MSG, SW_SHOWNORMAL, WM_CLOSE, WM_COMMAND, WM_INITDIALOG, WM_NULL, WM_TIMER, WS_BORDER,
    WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE, WS_VSCROLL,
};
use zeroize::Zeroizing;

use crate::codes::{Refusal, INVALID_INPUT, OWN_STATE};
use crate::preflight::Staged;
use crate::starter::Process;
use crate::wizard::{asks_again, Answer, Closed, Fields, Offer, Starter, Wizard};

/// The controls' identifiers.
pub const ID_SERVER_URL: i32 = 101;
pub const ID_HEARTBEAT_URL: i32 = 102;
pub const ID_ANCHOR_PATH: i32 = 103;
pub const ID_FINGERPRINTS: i32 = 104;
pub const ID_TOKEN: i32 = 105;
/// Where a refusal of a submission shows.
pub const ID_MESSAGE: i32 = 106;
pub const ID_INTRO: i32 = 107;
/// A pending host's choice to keep its configuration.
pub const ID_KEEP: i32 = 108;
/// The submission.
pub const ID_SUBMIT: i32 = IDOK;
pub const ID_CANCEL: i32 = IDCANCEL;

/// The 15 minutes of point 5.
pub const TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// How often the dialog reads the starter's state and the time.
pub const POLL: Duration = Duration::from_millis(250);

const TIMER: usize = 1;
const SS_LEFT: u32 = 0;
const SS_NOPREFIX: u32 = 0x80;

/// What the tests see of a dialog: when it shows, and when it closes, before
/// it is destroyed.
pub trait Observe {
    fn shown(&self, _dialog: HWND) {}
    fn closing(&self, _dialog: HWND) {}
}

/// No observer.
pub struct Unobserved;

impl Observe for Unobserved {}

/// How the dialog runs: the setup's is shown, with point 5's 15 minutes; the
/// tests' is hidden, with a time of their own.
pub struct Settings<'a> {
    pub visible: bool,
    pub timeout: Duration,
    pub poll: Duration,
    pub observe: &'a dyn Observe,
}

impl Settings<'static> {
    /// The setup's.
    pub const SETUP: Settings<'static> = Settings {
        visible: true,
        timeout: TIMEOUT,
        poll: POLL,
        observe: &Unobserved,
    };
}

/// The setup's wizard: the process that started it, and the dialog.
pub struct SystemWizard;

impl Wizard for SystemWizard {
    fn hold_starter(&self) -> Result<Box<dyn Starter>, Refusal> {
        Process::starter().map(|process| Box::new(process) as Box<dyn Starter>)
    }

    fn ask(
        &self,
        offer: Offer,
        starter: &dyn Starter,
        take: &mut dyn FnMut(&Fields) -> Result<Staged, Refusal>,
    ) -> Result<Answer, Refusal> {
        ask(offer, starter, take, &Settings::SETUP)
    }
}

// --- The template ------------------------------------------------------------

/// The dialog's title, apart from the setup's window, 'CyberGuard Agent Setup'.
pub const TITLE: &str = "CyberGuard Agent enrollment";
const INTRO_FRESH: &str = "Enter the values your CyberGuard administrator gave you to \
    install the agent on this computer. The fingerprints are those of the list you were \
    given, not of the file. The token is not shown.";
const INTRO_PENDING: &str = "This computer holds an agent configuration that has not \
    enrolled yet. Keep it, or replace it with the values your CyberGuard administrator \
    gave you. The token is not shown.";

const BUTTON: u16 = 0x0080;
const EDIT: u16 = 0x0081;
const STATIC: u16 = 0x0082;

/// A dialog template in memory: `DLGTEMPLATE`, its items, each
/// `DLGITEMTEMPLATE` on a 4-byte boundary.
struct Template {
    words: Vec<u16>,
    items: u16,
}

fn push_text(words: &mut Vec<u16>, text: &str) {
    words.extend(text.encode_utf16());
    words.push(0);
}

fn push_u32(words: &mut Vec<u16>, value: u32) {
    words.push(value as u16);
    words.push((value >> 16) as u16);
}

impl Template {
    fn new(style: u32, extended: u32, (cx, cy): (i16, i16), title: &str) -> Self {
        let mut words = Vec::new();
        push_u32(&mut words, style | DS_SETFONT as u32);
        push_u32(&mut words, extended);
        words.push(0); // the item count, set by `finish`
        for value in [0i16, 0, cx, cy] {
            words.push(value as u16);
        }
        words.push(0); // no menu
        words.push(0); // the dialog class
        push_text(&mut words, title);
        words.push(8); // the font's point size
        push_text(&mut words, "MS Shell Dlg");
        Template { words, items: 0 }
    }

    fn item(&mut self, class: u16, id: i32, style: u32, rect: (i16, i16, i16, i16), text: &str) {
        if self.words.len() % 2 == 1 {
            self.words.push(0);
        }
        push_u32(&mut self.words, style | WS_CHILD | WS_VISIBLE);
        push_u32(&mut self.words, 0);
        for value in [rect.0, rect.1, rect.2, rect.3] {
            self.words.push(value as u16);
        }
        self.words.push(id as u16);
        self.words.push(0xFFFF);
        self.words.push(class);
        push_text(&mut self.words, text);
        self.words.push(0); // no creation data
        self.items += 1;
    }

    /// The template's bytes, on a 4-byte boundary.
    fn finish(mut self) -> Vec<u32> {
        self.words[4] = self.items;
        if self.words.len() % 2 == 1 {
            self.words.push(0);
        }
        self.words
            .chunks(2)
            .map(|pair| u32::from(pair[0]) | (u32::from(pair[1]) << 16))
            .collect()
    }
}

fn template(offer: Offer) -> Vec<u32> {
    let mut t = Template::new(
        DS_MODALFRAME as u32 | DS_CENTER as u32 | WS_POPUP | WS_CAPTION | WS_SYSMENU,
        WS_EX_TOPMOST | WS_EX_APPWINDOW,
        (320, 250),
        TITLE,
    );
    let intro = match offer {
        Offer::Fresh => INTRO_FRESH,
        Offer::Pending => INTRO_PENDING,
    };
    let label = SS_LEFT | SS_NOPREFIX;
    let field = WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32;
    t.item(STATIC, ID_INTRO, label, (7, 7, 306, 26), intro);
    let rows: [(i32, &str, u32, i16); 5] = [
        (
            ID_SERVER_URL,
            "Enrollment server URL (server.url):",
            field,
            12,
        ),
        (
            ID_HEARTBEAT_URL,
            "Heartbeat URL (server.heartbeat_url):",
            field,
            12,
        ),
        (
            ID_ANCHOR_PATH,
            "Trust anchor file, its full path (server.trust_anchor_path):",
            field,
            12,
        ),
        (
            ID_FINGERPRINTS,
            "Trust anchor fingerprints, one per line (setup.trust_anchor_sha256):",
            WS_BORDER
                | WS_TABSTOP
                | WS_VSCROLL
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_WANTRETURN as u32,
            34,
        ),
        (
            ID_TOKEN,
            "Enrollment token (enrollment.token):",
            field | ES_PASSWORD as u32,
            12,
        ),
    ];
    let mut y = 36;
    for (id, text, style, height) in rows {
        t.item(STATIC, -1, label, (7, y, 306, 8), text);
        t.item(EDIT, id, style, (7, y + 9, 306, height), "");
        y += 9 + height + 6;
    }
    // Room for four lines of a refusal, above the buttons.
    t.item(STATIC, ID_MESSAGE, label, (7, y, 306, 32), "");
    let buttons = y + 36;
    if offer == Offer::Pending {
        t.item(
            BUTTON,
            ID_KEEP,
            BS_PUSHBUTTON as u32 | WS_TABSTOP,
            (7, buttons, 110, 14),
            "&Keep the current configuration",
        );
    }
    t.item(
        BUTTON,
        ID_SUBMIT,
        BS_DEFPUSHBUTTON as u32 | WS_TABSTOP,
        (198, buttons, 55, 14),
        "&Install",
    );
    t.item(
        BUTTON,
        ID_CANCEL,
        BS_PUSHBUTTON as u32 | WS_TABSTOP,
        (258, buttons, 55, 14),
        "Cancel",
    );
    t.finish()
}

// --- The dialog --------------------------------------------------------------

/// What the dialog procedure reaches, through the window's user data.
struct State<'a> {
    offer: Offer,
    starter: &'a dyn Starter,
    take: &'a mut dyn FnMut(&Fields) -> Result<Staged, Refusal>,
    deadline: Instant,
    observe: &'a dyn Observe,
    outcome: Option<Result<Answer, Refusal>>,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// A control's text, in memory zeroed after use; `None` if it is not valid
/// UTF-16.
fn read(dialog: HWND, id: i32) -> Option<Zeroizing<String>> {
    // SAFETY: GetDlgItem takes any identifier; a null control has no text.
    let control = unsafe { GetDlgItem(dialog, id) };
    // SAFETY: a window handle, or null.
    let length = unsafe { GetWindowTextLengthW(control) }.max(0) as usize;
    let mut buffer = Zeroizing::new(vec![0u16; length + 1]);
    // SAFETY: the buffer holds `length + 1` UTF-16 units.
    let copied = unsafe { GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32) };
    let copied = copied.max(0) as usize;
    String::from_utf16(&buffer[..copied.min(length)])
        .ok()
        .map(Zeroizing::new)
}

fn set_text(dialog: HWND, id: i32, text: &str) {
    let text = wide(text);
    // SAFETY: the string lives for the call.
    unsafe { SetWindowTextW(GetDlgItem(dialog, id), text.as_ptr()) };
}

/// The token's field emptied, its text first overwritten with as many
/// characters, so that the control's buffer of that length holds no token
/// (the control's memory is not the setup's to zero, point 6).
fn clear_token(dialog: HWND) {
    // SAFETY: GetDlgItem takes any identifier.
    let length = unsafe { GetWindowTextLengthW(GetDlgItem(dialog, ID_TOKEN)) }.max(0) as usize;
    set_text(dialog, ID_TOKEN, &"0".repeat(length));
    set_text(dialog, ID_TOKEN, "");
}

/// Close the dialog with `outcome`: its token field cleared, then destroyed;
/// a message posted to the thread wakes `ask`'s loop, which a message sent
/// from another thread, handled within GetMessageW, would not.
fn close(dialog: HWND, state: &mut State<'_>, outcome: Result<Answer, Refusal>) {
    clear_token(dialog);
    state.outcome = Some(outcome);
    state.observe.closing(dialog);
    // SAFETY: the dialog's own handle, on its own thread.
    unsafe {
        KillTimer(dialog, TIMER);
        DestroyWindow(dialog);
        PostThreadMessageW(GetCurrentThreadId(), WM_NULL, 0, 0);
    }
}

/// A submission: the fields read, checked by `take`, then the token's memory
/// zeroed.
fn submit(dialog: HWND, state: &mut State<'_>) {
    let unreadable = |field: &str| {
        Refusal::new(
            INVALID_INPUT,
            format!("invalid input: {field} is not valid text"),
        )
    };
    let fields = (|| {
        Ok(Fields {
            server_url: read(dialog, ID_SERVER_URL)
                .ok_or_else(|| unreadable("server.url"))?
                .to_string(),
            heartbeat_url: read(dialog, ID_HEARTBEAT_URL)
                .ok_or_else(|| unreadable("server.heartbeat_url"))?
                .to_string(),
            anchor_path: read(dialog, ID_ANCHOR_PATH)
                .ok_or_else(|| unreadable("server.trust_anchor_path"))?
                .to_string(),
            fingerprints: read(dialog, ID_FINGERPRINTS)
                .ok_or_else(|| unreadable("setup.trust_anchor_sha256"))?
                .to_string(),
            token: read(dialog, ID_TOKEN).ok_or_else(|| unreadable("enrollment.token"))?,
        })
    })();
    let taken = fields.and_then(|mut fields| {
        let taken = (state.take)(&fields);
        fields.clear_token();
        taken
    });
    match taken {
        Ok(staged) => close(dialog, state, Ok(Answer::Input(staged))),
        Err(refusal) if asks_again(&refusal) => {
            let text = refusal
                .line
                .strip_prefix("cg-setup: ")
                .unwrap_or(&refusal.line)
                .to_string();
            set_text(dialog, ID_MESSAGE, &text);
        }
        Err(refusal) => close(dialog, state, Err(refusal)),
    }
}

/// The dialog's procedure. Only the buttons' clicks, the timer and the close
/// reach the state: a control's notification sent while a handler runs (a
/// field's change, the destruction) does not, so the state is never reached
/// twice at once.
unsafe extern "system" fn procedure(
    dialog: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    if message == WM_INITDIALOG {
        // SAFETY: `lparam` is the State that `ask` passed, which outlives the
        // dialog.
        unsafe { SetWindowLongPtrW(dialog, GWLP_USERDATA, lparam) };
        return 1;
    }
    let handled = match message {
        WM_COMMAND => {
            let (id, code) = ((wparam & 0xFFFF) as i32, ((wparam >> 16) & 0xFFFF) as u32);
            code == BN_CLICKED && matches!(id, ID_SUBMIT | ID_CANCEL | ID_KEEP)
        }
        WM_TIMER => wparam == TIMER,
        WM_CLOSE => true,
        _ => false,
    };
    if !handled {
        return 0;
    }
    // SAFETY: set at WM_INITDIALOG to the live State; null before it.
    let state = unsafe { GetWindowLongPtrW(dialog, GWLP_USERDATA) } as *mut State<'_>;
    // SAFETY: the State outlives the dialog and only this procedure, on the
    // dialog's thread, reaches it, once at a time (above).
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    if state.outcome.is_some() {
        return 1;
    }
    match message {
        WM_COMMAND => match (wparam & 0xFFFF) as i32 {
            ID_SUBMIT => submit(dialog, state),
            ID_KEEP if state.offer == Offer::Pending => close(dialog, state, Ok(Answer::Keep)),
            ID_KEEP => {}
            _ => close(dialog, state, Ok(Answer::Closed(Closed::Cancelled))),
        },
        WM_TIMER => {
            if !state.starter.running() {
                close(dialog, state, Ok(Answer::Closed(Closed::StarterExited)));
            } else if Instant::now() >= state.deadline {
                close(dialog, state, Ok(Answer::Closed(Closed::TimedOut)));
            }
        }
        _ => close(dialog, state, Ok(Answer::Closed(Closed::Cancelled))),
    }
    1
}

/// Show the dialog for `offer` on this thread until it closes (the
/// `Wizard::ask` of the setup, with `settings`). 40 if it cannot be created.
pub fn ask(
    offer: Offer,
    starter: &dyn Starter,
    take: &mut dyn FnMut(&Fields) -> Result<Staged, Refusal>,
    settings: &Settings<'_>,
) -> Result<Answer, Refusal> {
    let template = template(offer);
    // Every access to the State, here and in the procedure, is through this
    // one pointer, and the procedure's only while this loop dispatches.
    let state = Box::into_raw(Box::new(State {
        offer,
        starter,
        take,
        deadline: Instant::now() + settings.timeout,
        observe: settings.observe,
        outcome: None,
    }));
    // SAFETY: the template is a well-formed DLGTEMPLATE on a 4-byte boundary,
    // and the State lives until it is taken back below, after the dialog is
    // destroyed.
    let dialog = unsafe {
        CreateDialogIndirectParamW(
            GetModuleHandleW(std::ptr::null()),
            template.as_ptr().cast(),
            std::ptr::null_mut(),
            Some(procedure),
            state as LPARAM,
        )
    };
    if dialog.is_null() {
        let error = std::io::Error::last_os_error();
        // SAFETY: from `Box::into_raw` above; no dialog holds it.
        drop(unsafe { Box::from_raw(state) });
        return Err(Refusal::new(
            OWN_STATE,
            format!("the wizard's dialog cannot be created: {error}"),
        ));
    }
    let poll = settings.poll.as_millis().clamp(10, u128::from(u32::MAX)) as u32;
    // SAFETY: the dialog's own handle; no message is dispatched until the
    // loop, so the State is not reached meanwhile.
    unsafe {
        SetTimer(dialog, TIMER, poll, None);
        if settings.visible {
            ShowWindow(dialog, SW_SHOWNORMAL);
            if SetForegroundWindow(dialog) == 0 {
                let flash = FLASHWINFO {
                    cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                    hwnd: dialog,
                    dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
                    uCount: 0,
                    dwTimeout: 0,
                };
                FlashWindowEx(&flash);
            }
        }
        // The 15 minutes count from its showing (point 5).
        (*state).deadline = Instant::now() + settings.timeout;
    }
    settings.observe.shown(dialog);
    // SAFETY: an all-zero MSG is a valid out-buffer.
    let mut message: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: the State is alive; only `close` sets its outcome, and only
    // `close` destroys the dialog, so it is alive while none is set.
    while unsafe { (*state).outcome.is_none() } {
        // SAFETY: the out-buffer is one MSG.
        let got = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        // SAFETY: as above.
        if unsafe { (*state).outcome.is_some() } {
            // A message sent from another thread closed it within GetMessageW.
            break;
        }
        if got <= 0 {
            // A WM_QUIT, or a failure: the dialog closes as cancelled.
            // SAFETY: no message is being dispatched, so nothing else reaches
            // the State.
            close(
                dialog,
                unsafe { &mut *state },
                Ok(Answer::Closed(Closed::Cancelled)),
            );
            break;
        }
        // SAFETY: as above.
        unsafe {
            if IsDialogMessageW(dialog, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    // SAFETY: from `Box::into_raw` above; the dialog is destroyed.
    let mut state = unsafe { Box::from_raw(state) };
    state
        .outcome
        .take()
        .expect("the dialog closed with an outcome")
}
