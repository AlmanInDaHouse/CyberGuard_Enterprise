//! The wizard's dialog (SPEC-024 Amendment 2026-11-13, points 4 to 7, and
//! its *Tests*): created hidden in the test's own process, on a thread of its
//! own, and driven from the test's thread by its controls' messages; it acts
//! on nothing outside this process. The starting process is a stand-in the
//! test sets, and the 15 minutes an injected time.
#![cfg(windows)]

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cg_setup::codes;
use cg_setup::dialog::{
    self, Observe, Settings, ID_ANCHOR_PATH, ID_CANCEL, ID_FINGERPRINTS, ID_HEARTBEAT_URL,
    ID_INTRO, ID_KEEP, ID_MESSAGE, ID_SERVER_URL, ID_SUBMIT, ID_TOKEN,
};
use cg_setup::preflight::Staged;
use cg_setup::wizard::{Answer, Closed, Fields, Offer, Starter};
use cg_setup::Refusal;
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, WPARAM};
use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetDlgCtrlID, GetDlgItem, GetWindowLongPtrW, GetWindowRect,
    SendMessageW, BN_CLICKED, ES_MULTILINE, ES_PASSWORD, GWL_EXSTYLE, GWL_STYLE, WM_CLOSE,
    WM_COMMAND, WM_COPY, WM_CUT, WM_GETTEXT, WM_GETTEXTLENGTH, WM_SETTEXT, WS_EX_TOPMOST,
};
use zeroize::Zeroizing;

const TOKEN: &str = "s71-dialog-enrollment-token-QUJDREVGR0g";
const EM_SETSEL: u32 = 0x00B1;
const EM_GETPASSWORDCHAR: u32 = 0x00D2;
const FIELDS: [i32; 5] = [
    ID_SERVER_URL,
    ID_HEARTBEAT_URL,
    ID_ANCHOR_PATH,
    ID_FINGERPRINTS,
    ID_TOKEN,
];

/// The starting process's stand-in.
struct StandIn(Arc<AtomicBool>);

impl Starter for StandIn {
    fn running(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A dialog's handle, as the test's thread holds it.
#[derive(Clone, Copy)]
struct Dialog(isize);

impl Dialog {
    fn hwnd(self) -> HWND {
        self.0 as HWND
    }

    fn control(self, id: i32) -> HWND {
        // SAFETY: GetDlgItem takes any identifier.
        unsafe { GetDlgItem(self.hwnd(), id) }
    }

    fn send(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
        // SAFETY: each message below takes what it is given; the dialog's
        // thread processes it while this one waits.
        unsafe { SendMessageW(window, message, wparam, lparam) }
    }

    fn set(self, id: i32, text: &str) {
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        Self::send(self.control(id), WM_SETTEXT, 0, wide.as_ptr() as LPARAM);
    }

    fn text(self, id: i32) -> String {
        text_of(self.control(id))
    }

    fn click(self, id: i32) {
        let wparam = (id as u32 as usize & 0xFFFF) | ((BN_CLICKED as usize) << 16);
        Self::send(self.hwnd(), WM_COMMAND, wparam, self.control(id) as LPARAM);
    }

    fn style(self, id: i32) -> u32 {
        // SAFETY: a window handle.
        unsafe { GetWindowLongPtrW(self.control(id), GWL_STYLE) as u32 }
    }

    /// A control's rectangle on the screen: (left, top, right, bottom).
    fn rect(self, id: i32) -> (i32, i32, i32, i32) {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: a window handle and one RECT.
        assert_ne!(
            unsafe { GetWindowRect(self.control(id), &mut rect) },
            0,
            "{id}"
        );
        (rect.left, rect.top, rect.right, rect.bottom)
    }

    fn fill(self, fields: &[(i32, &str)]) {
        for (id, text) in fields {
            self.set(*id, text);
        }
    }
}

fn text_of(window: HWND) -> String {
    let length = Dialog::send(window, WM_GETTEXTLENGTH, 0, 0).max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    let copied = Dialog::send(
        window,
        WM_GETTEXT,
        buffer.len(),
        buffer.as_mut_ptr() as LPARAM,
    )
    .max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied])
}

/// What the dialog's thread reports: its handle when it shows, and each
/// field's text when it closes.
struct Watcher {
    shown: Mutex<mpsc::Sender<isize>>,
    closing: Mutex<Option<Vec<String>>>,
}

impl Observe for Watcher {
    fn shown(&self, dialog: HWND) {
        self.shown.lock().unwrap().send(dialog as isize).unwrap();
    }

    fn closing(&self, dialog: HWND) {
        // SAFETY: GetDlgItem takes any identifier.
        let texts = FIELDS
            .iter()
            .map(|id| text_of(unsafe { GetDlgItem(dialog, *id) }))
            .collect();
        *self.closing.lock().unwrap() = Some(texts);
    }
}

/// A staged input the test's `take` returns, told apart by `marker`.
fn staged(marker: &str) -> Staged {
    Staged {
        config: Zeroizing::new(String::new()),
        anchor: Vec::new(),
        fingerprints: BTreeSet::from([marker.to_string()]),
    }
}

/// The fields as `take` received them, the token included.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Taken {
    server_url: String,
    heartbeat_url: String,
    anchor_path: String,
    fingerprints: String,
    token: String,
}

impl Taken {
    fn of(fields: &Fields) -> Self {
        Taken {
            server_url: fields.server_url.clone(),
            heartbeat_url: fields.heartbeat_url.clone(),
            anchor_path: fields.anchor_path.clone(),
            fingerprints: fields.fingerprints.clone(),
            token: fields.token.to_string(),
        }
    }
}

/// What a run of the dialog gave.
struct Run {
    outcome: Result<Answer, Refusal>,
    /// Each field's text as the dialog closed.
    closing: Vec<String>,
    taken: Vec<Taken>,
    elapsed: Duration,
}

/// Run the dialog for `offer` on a thread of its own, hidden, with `timeout`
/// and the stand-in `running`; `answers` gives `take`'s answer to each
/// submission in turn; `drive` acts on the dialog from this thread.
fn show(
    offer: Offer,
    timeout: Duration,
    running: Arc<AtomicBool>,
    answers: Vec<Result<Staged, Refusal>>,
    drive: impl FnOnce(Dialog),
) -> Run {
    let (sender, receiver) = mpsc::channel();
    let watcher = Watcher {
        shown: Mutex::new(sender),
        closing: Mutex::new(None),
    };
    let taken = Mutex::new(Vec::new());
    let started = Instant::now();
    let outcome = std::thread::scope(|scope| {
        let thread = scope.spawn(|| {
            let mut answers = answers.into_iter();
            let mut take = |fields: &Fields| {
                taken.lock().unwrap().push(Taken::of(fields));
                answers.next().expect("an answer for each submission")
            };
            let settings = Settings {
                visible: false,
                timeout,
                poll: Duration::from_millis(50),
                observe: &watcher,
            };
            dialog::ask(offer, &StandIn(running), &mut take, &settings)
        });
        let dialog = Dialog(
            receiver
                .recv_timeout(Duration::from_secs(30))
                .expect("the dialog shows"),
        );
        let driven = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drive(dialog)));
        if driven.is_err() {
            // A failed assertion: close the dialog so that its thread ends.
            Dialog::send(dialog.hwnd(), WM_CLOSE, 0, 0);
        }
        let outcome = thread.join().expect("the dialog's thread");
        if let Err(panic) = driven {
            std::panic::resume_unwind(panic);
        }
        outcome
    });
    Run {
        outcome,
        closing: watcher.closing.into_inner().unwrap().expect("it closed"),
        taken: taken.into_inner().unwrap(),
        elapsed: started.elapsed(),
    }
}

fn running() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(true))
}

const MINUTE: Duration = Duration::from_secs(60);

fn valid() -> [(i32, &'static str); 5] {
    [
        (ID_SERVER_URL, "https://enroll.example.test:8444"),
        (ID_HEARTBEAT_URL, "https://ingest.example.test:8443"),
        (ID_ANCHOR_PATH, r"C:\operator\server-ca.pem"),
        (
            ID_FINGERPRINTS,
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n\
             sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ),
        (ID_TOKEN, TOKEN),
    ]
}

fn the_token_is_cleared(run: &Run) {
    assert_eq!(
        run.closing[4], "",
        "the token's field is empty as it closes"
    );
}

// --- The fields ----------------------------------------------------------------

/// The fields show empty; the token's field is masked and single-line and
/// refuses copy and cut; the dialog is topmost; a fresh host has no choice
/// to keep.
#[test]
fn package_dialog_the_token_field_is_masked_single_line_and_refuses_copy_and_cut() {
    let checked = Mutex::new(false);
    let run = show(Offer::Fresh, MINUTE, running(), vec![], |dialog| {
        for id in FIELDS {
            assert_eq!(dialog.text(id), "", "{id} is not pre-filled");
        }
        let style = dialog.style(ID_TOKEN);
        assert_ne!(style & ES_PASSWORD as u32, 0, "masked");
        assert_eq!(style & ES_MULTILINE as u32, 0, "single-line");
        let token = dialog.control(ID_TOKEN);
        assert_ne!(Dialog::send(token, EM_GETPASSWORDCHAR, 0, 0), 0);
        // SAFETY: a window handle.
        let extended = unsafe { GetWindowLongPtrW(dialog.hwnd(), GWL_EXSTYLE) } as u32;
        assert_ne!(extended & WS_EX_TOPMOST, 0, "topmost");
        assert!(dialog.control(ID_KEEP).is_null(), "no choice to keep");
        assert_eq!(text_of(dialog.hwnd()), dialog::TITLE);
        assert_eq!(dialog::TITLE, "CyberGuard Agent enrollment");

        dialog.set(ID_TOKEN, TOKEN);
        Dialog::send(token, EM_SETSEL, 0, -1);
        // SAFETY: reads the clipboard's sequence number and nothing else.
        let before = unsafe { GetClipboardSequenceNumber() };
        Dialog::send(token, WM_COPY, 0, 0);
        Dialog::send(token, WM_CUT, 0, 0);
        // SAFETY: as above.
        let after = unsafe { GetClipboardSequenceNumber() };
        assert_eq!(before, after, "nothing reached the clipboard");
        assert_eq!(dialog.text(ID_TOKEN), TOKEN, "the cut removed nothing");
        *checked.lock().unwrap() = true;
        dialog.click(ID_CANCEL);
    });
    assert!(*checked.lock().unwrap());
    assert!(matches!(run.outcome, Ok(Answer::Closed(Closed::Cancelled))));
    the_token_is_cleared(&run);
    assert!(run.taken.is_empty());
}

/// A valid submission gives the fields' input, as typed, and the answer
/// `take` gave; the token's field is empty as the dialog closes.
#[test]
fn package_dialog_a_valid_submission_gives_the_fields_input() {
    let run = show(
        Offer::Fresh,
        MINUTE,
        running(),
        vec![Ok(staged("accepted"))],
        |dialog| {
            dialog.fill(&valid());
            dialog.click(ID_SUBMIT);
        },
    );
    let [server, heartbeat, anchor, fingerprints, token] = valid().map(|(_, text)| text);
    assert_eq!(
        run.taken,
        vec![Taken {
            server_url: server.into(),
            heartbeat_url: heartbeat.into(),
            anchor_path: anchor.into(),
            fingerprints: fingerprints.into(),
            token: token.into(),
        }]
    );
    match &run.outcome {
        Ok(Answer::Input(staged)) => {
            assert_eq!(
                staged.fingerprints,
                BTreeSet::from(["accepted".to_string()])
            )
        }
        other => panic!("the input was expected: {other:?}"),
    }
    the_token_is_cleared(&run);
}

/// A refusal of a field (24) or of the anchor (25) shows in the dialog,
/// without the line's prefix, and the dialog asks again with the fields as
/// they were, the token kept; any other refusal closes it with its code.
#[test]
fn package_dialog_a_refused_field_is_shown_and_asked_again() {
    let field = Refusal::new(
        codes::INVALID_INPUT,
        "invalid input: the configuration generated from the input is refused: server.url",
    );
    let anchor = Refusal::new(
        codes::ANCHOR,
        r"trust anchor: open C:\operator\server-ca.pem",
    );
    let run = show(
        Offer::Fresh,
        MINUTE,
        running(),
        vec![Err(field), Err(anchor), Ok(staged("third"))],
        |dialog| {
            dialog.fill(&valid());
            dialog.click(ID_SUBMIT);
            assert_eq!(
                dialog.text(ID_MESSAGE),
                "invalid input: the configuration generated from the input is refused: server.url"
            );
            for (id, text) in valid() {
                assert_eq!(dialog.text(id), text, "{id} as it was");
            }
            dialog.click(ID_SUBMIT);
            assert_eq!(
                dialog.text(ID_MESSAGE),
                r"trust anchor: open C:\operator\server-ca.pem"
            );
            assert_eq!(dialog.text(ID_TOKEN), TOKEN, "the token kept");
            dialog.click(ID_SUBMIT);
        },
    );
    assert_eq!(run.taken.len(), 3);
    assert!(matches!(run.outcome, Ok(Answer::Input(_))));
    the_token_is_cleared(&run);

    let run = show(
        Offer::Fresh,
        MINUTE,
        running(),
        vec![Err(Refusal::new(
            codes::OWN_STATE,
            "the host cannot be read",
        ))],
        |dialog| {
            dialog.fill(&valid());
            dialog.click(ID_SUBMIT);
        },
    );
    match &run.outcome {
        Err(refusal) => assert_eq!(refusal.code, codes::OWN_STATE),
        other => panic!("40 was expected: {other:?}"),
    }
    the_token_is_cleared(&run);
}

// --- Closing -------------------------------------------------------------------

/// Cancel, the window's close, the time and the starter's exit each close
/// the dialog with no submission taken and the token's field empty.
#[test]
fn package_dialog_cancel_close_time_and_the_starters_exit_close_it() {
    let cancelled = show(Offer::Fresh, MINUTE, running(), vec![], |dialog| {
        dialog.set(ID_TOKEN, TOKEN);
        dialog.click(ID_CANCEL);
    });
    assert!(matches!(
        cancelled.outcome,
        Ok(Answer::Closed(Closed::Cancelled))
    ));

    let closed = show(Offer::Pending, MINUTE, running(), vec![], |dialog| {
        dialog.set(ID_TOKEN, TOKEN);
        Dialog::send(dialog.hwnd(), WM_CLOSE, 0, 0);
    });
    assert!(matches!(
        closed.outcome,
        Ok(Answer::Closed(Closed::Cancelled))
    ));

    let timed = show(
        Offer::Fresh,
        Duration::from_secs(1),
        running(),
        vec![],
        |dialog| dialog.set(ID_TOKEN, TOKEN),
    );
    assert!(matches!(
        timed.outcome,
        Ok(Answer::Closed(Closed::TimedOut))
    ));
    assert!(
        timed.elapsed >= Duration::from_secs(1),
        "{:?}",
        timed.elapsed
    );

    let starter = running();
    let exited = show(Offer::Fresh, MINUTE, starter.clone(), vec![], |dialog| {
        dialog.set(ID_TOKEN, TOKEN);
        starter.store(false, Ordering::SeqCst);
    });
    assert!(matches!(
        exited.outcome,
        Ok(Answer::Closed(Closed::StarterExited))
    ));
    assert!(
        exited.elapsed < Duration::from_secs(30),
        "{:?}",
        exited.elapsed
    );

    for run in [cancelled, closed, timed, exited] {
        assert!(run.taken.is_empty());
        the_token_is_cleared(&run);
    }
}

/// The setup's dialog shows, for 15 minutes (point 5); the tests' are
/// hidden with times of their own.
#[test]
fn package_dialog_the_setups_dialog_shows_for_15_minutes() {
    assert_eq!(dialog::TIMEOUT, Duration::from_secs(15 * 60));
    let setup = Settings::SETUP;
    assert!(setup.visible);
    assert_eq!(setup.timeout, dialog::TIMEOUT);
}

/// A refused submission does not restart the time.
#[test]
fn package_dialog_a_refused_submission_does_not_restart_the_time() {
    let timeout = Duration::from_secs(3);
    let run = show(
        Offer::Fresh,
        timeout,
        running(),
        vec![Err(Refusal::new(
            codes::INVALID_INPUT,
            "invalid input: server.url",
        ))],
        |dialog| {
            std::thread::sleep(Duration::from_secs(2));
            dialog.fill(&valid());
            dialog.click(ID_SUBMIT);
        },
    );
    assert!(matches!(run.outcome, Ok(Answer::Closed(Closed::TimedOut))));
    assert_eq!(run.taken.len(), 1);
    assert!(
        run.elapsed < Duration::from_millis(4500),
        "closed {:?} after it showed, not 3 s",
        run.elapsed
    );
}

/// A pending host's dialog offers to keep its configuration; no control
/// overlaps another, a refusal's text above the buttons.
#[test]
fn package_dialog_a_pending_host_offers_to_keep() {
    let run = show(Offer::Pending, MINUTE, running(), vec![], |dialog| {
        assert!(!dialog.control(ID_KEEP).is_null());
        let ids = [
            ID_INTRO,
            ID_SERVER_URL,
            ID_HEARTBEAT_URL,
            ID_ANCHOR_PATH,
            ID_FINGERPRINTS,
            ID_TOKEN,
            ID_MESSAGE,
        ];
        for pair in ids.windows(2) {
            assert!(
                dialog.rect(pair[0]).3 <= dialog.rect(pair[1]).1,
                "{} above {}",
                pair[0],
                pair[1]
            );
        }
        for button in [ID_KEEP, ID_SUBMIT, ID_CANCEL] {
            assert!(
                dialog.rect(ID_MESSAGE).3 <= dialog.rect(button).1,
                "{button}"
            );
        }
        assert!(dialog.text(ID_INTRO).contains("has not enrolled"));
        dialog.set(ID_TOKEN, TOKEN);
        dialog.click(ID_KEEP);
    });
    assert!(matches!(run.outcome, Ok(Answer::Keep)));
    assert!(run.taken.is_empty());
    the_token_is_cleared(&run);
}

// --- What the dialog holds --------------------------------------------------------

unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
    // SAFETY: `list` is the Vec `controls` passes, for the call.
    unsafe { (*(list as *mut Vec<isize>)).push(window as isize) };
    1
}

/// Each control of the dialog: its class and its identifier.
fn controls(dialog: Dialog) -> Vec<(String, i32)> {
    let mut list: Vec<isize> = Vec::new();
    // SAFETY: the callback writes into `list`, which outlives the call.
    unsafe {
        EnumChildWindows(
            dialog.hwnd(),
            Some(collect),
            std::ptr::addr_of_mut!(list) as LPARAM,
        )
    };
    list.into_iter()
        .map(|window| {
            let mut class = [0u16; 64];
            // SAFETY: a window handle and a buffer of 64 units.
            let length = unsafe { GetClassNameW(window as HWND, class.as_mut_ptr(), 64) };
            // SAFETY: a window handle.
            let id = unsafe { GetDlgCtrlID(window as HWND) };
            (
                String::from_utf16_lossy(&class[..length.max(0) as usize]),
                id,
            )
        })
        .collect()
}

/// The dialog's controls are its five fields, its buttons and its texts
/// alone: no control shows the token and none picks a file (points 4 and 6).
#[test]
fn package_dialog_its_controls_are_the_fields_the_buttons_and_texts_alone() {
    for (offer, buttons) in [
        (Offer::Fresh, vec![ID_SUBMIT, ID_CANCEL]),
        (Offer::Pending, vec![ID_KEEP, ID_SUBMIT, ID_CANCEL]),
    ] {
        let seen = Mutex::new(Vec::new());
        show(offer, MINUTE, running(), vec![], |dialog| {
            *seen.lock().unwrap() = controls(dialog);
            dialog.click(ID_CANCEL);
        });
        let seen = seen.into_inner().unwrap();
        let of = |class: &str| -> Vec<i32> {
            let mut ids: Vec<i32> = seen
                .iter()
                .filter(|(c, _)| c.eq_ignore_ascii_case(class))
                .map(|(_, id)| *id)
                .collect();
            ids.sort();
            ids
        };
        let mut fields = FIELDS.to_vec();
        fields.sort();
        assert_eq!(of("Edit"), fields, "{offer:?}: the five fields");
        let mut buttons = buttons;
        buttons.sort();
        assert_eq!(of("Button"), buttons, "{offer:?}: its buttons");
        assert!(
            seen.iter().all(|(class, _)| ["Edit", "Button", "Static"]
                .iter()
                .any(|known| class.eq_ignore_ascii_case(known))),
            "{offer:?}: no other control: {seen:?}"
        );
    }
}

/// A token of any text is taken as typed; one that is not valid UTF-16 is
/// refused in the dialog, naming its field, and asked again.
#[test]
fn package_dialog_a_token_is_read_as_typed_or_refused_as_invalid_text() {
    let typed = "tökén-ñ-€-𝄞-QUJD";
    let run = show(
        Offer::Fresh,
        MINUTE,
        running(),
        vec![Ok(staged("typed"))],
        |dialog| {
            dialog.fill(&valid());
            // A lone surrogate, which no string holds.
            let invalid: Vec<u16> = vec![0x0041, 0xD800, 0x0042, 0];
            Dialog::send(
                dialog.control(ID_TOKEN),
                WM_SETTEXT,
                0,
                invalid.as_ptr() as LPARAM,
            );
            dialog.click(ID_SUBMIT);
            assert_eq!(
                dialog.text(ID_MESSAGE),
                "invalid input: enrollment.token is not valid text"
            );
            dialog.set(ID_TOKEN, typed);
            dialog.click(ID_SUBMIT);
        },
    );
    assert_eq!(run.taken.len(), 1, "the invalid token reached no check");
    assert_eq!(run.taken[0].token, typed);
    assert!(matches!(run.outcome, Ok(Answer::Input(_))));
    the_token_is_cleared(&run);
}
