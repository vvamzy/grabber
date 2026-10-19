#![windows_subsystem = "windows"] // GUI app — no console window

mod api;

use api::{DailyStat, Download};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use windows::core::{w, PCWSTR, PWSTR, Result};
use windows::Win32::Foundation::{BOOL, COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontIndirectW, CreatePen, CreateSolidBrush,
    DeleteDC, DeleteObject, Ellipse, FillRect, GetDC, GetStockObject, LineTo, MoveToEx, Polygon,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, COLOR_WINDOW, DEFAULT_GUI_FONT,
    DIB_RGB_COLORS, FW_BOLD, HBITMAP, HBRUSH, HDC, HGDIOBJ, LOGFONTW, NULL_BRUSH, PS_SOLID,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, CDRF_DODEFAULT, CDRF_NEWFONT, CDRF_NOTIFYITEMDRAW,
    CDRF_NOTIFYSUBITEMDRAW, CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDDS_SUBITEM, EM_SETCUEBANNER,
    EM_SETSEL, ICC_BAR_CLASSES, ICC_COOL_CLASSES, ICC_LISTVIEW_CLASSES, ICC_PROGRESS_CLASS,
    ICC_TREEVIEW_CLASSES, INITCOMMONCONTROLSEX, LVCOLUMNW, LVCF_TEXT, LVCF_WIDTH, LVIF_STATE,
    LVIF_TEXT, LVITEMW, LVS_EX_DOUBLEBUFFER, LVS_EX_FULLROWSELECT, LVS_REPORT, LVS_SHOWSELALWAYS,
    LVM_DELETEALLITEMS, LVM_ENSUREVISIBLE, LVM_GETITEMCOUNT, LVM_GETNEXTITEM, LVM_INSERTCOLUMNW,
    LVM_INSERTITEMW, LVM_SETCOLUMNWIDTH, LVM_SETEXTENDEDLISTVIEWSTYLE, LVM_SETITEMSTATE,
    LVM_SETITEMTEXTW, LVS_EX_GRIDLINES, LVIS_SELECTED, LVNI_SELECTED, NM_CLICK, NM_CUSTOMDRAW,
    NM_DBLCLK, NMHDR, NMLVCUSTOMDRAW, NMTREEVIEWW, PBM_SETPOS, PBM_SETRANGE32, PBS_SMOOTH,
    RBBS_CHILDEDGE, RBBIM_CHILD, RBBIM_CHILDSIZE, RBBIM_SIZE, RBBIM_STYLE, RB_INSERTBANDW,
    REBARBANDINFOW, SBARS_SIZEGRIP, SB_SETPARTS, SB_SETTEXTW, TBSTATE_ENABLED,
    TBSTYLE_EX_DOUBLEBUFFER, TBSTYLE_FLAT, TBSTYLE_TOOLTIPS, TB_ADDBUTTONSW, TB_AUTOSIZE,
    TB_BUTTONSTRUCTSIZE, TB_SETEXTENDEDSTYLE, TB_SETBUTTONSIZE, TB_SETIMAGELIST, TBBUTTON,
    BTNS_BUTTON, BTNS_SEP, TVIF_IMAGE, TVIF_SELECTEDIMAGE, TVIF_TEXT, TVI_LAST, TVI_ROOT,
    TVINSERTSTRUCTW, TVINSERTSTRUCTW_0, TVITEMW, TVM_INSERTITEMW, TVM_SETBKCOLOR,
    TVM_SETIMAGELIST, TVM_SETITEMW, TVM_SETTEXTCOLOR, TVN_SELCHANGEDW, TVSIL_NORMAL,
    TVS_HASBUTTONS, TVS_HASLINES, TVS_LINESATROOT, TVS_SHOWSELALWAYS, HTREEITEM, ILC_COLOR32,
    ILD_TRANSPARENT, ImageList_Add, ImageList_Create, ImageList_GetIcon, HIMAGELIST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------

// Child controls of the main window.
const IDC_LIST: u16 = 1200;
const IDC_TREE: u16 = 1201;
const IDC_STATUS: u16 = 1202;
const IDC_SEARCH: u16 = 1203;
const IDC_CATHEAD: u16 = 1204;

// Add-URL popup controls.
const IDC_URL: u16 = 1300;
const IDC_SEGS: u16 = 1301;

// Progress-window controls.
const IDC_P_FILE: u16 = 1400;
const IDC_P_DOWN: u16 = 1401;
const IDC_P_BAR: u16 = 1402;
const IDC_P_RATE: u16 = 1403;
const IDC_P_TIME: u16 = 1404;
const IDC_P_STATUS: u16 = 1405;
const IDC_P_SEG: u16 = 1406;
const IDC_P_STOP: u16 = 1407;
const IDC_P_REMOVE: u16 = 1408;
const IDC_P_CLOSE: u16 = 1409;

// Statistics window list.
const IDC_STATLIST: u16 = 1500;

// Menu / toolbar commands.
const IDM_ADDURL: u16 = 40001;
const IDM_EXIT: u16 = 40002;
const IDM_DOWNLOADNOW: u16 = 40010; // a.k.a. Resume / Redownload
const IDM_REDOWNLOAD: u16 = 40011;
const IDM_STOPDL: u16 = 40012;
const IDM_PAUSEALL: u16 = 40013; // grayed — no pause in service
const IDM_STOPALL: u16 = 40014;
const IDM_DELETECOMPLETED: u16 = 40015;
const IDM_REMOVESEL: u16 = 40016;
const IDM_STATISTICS: u16 = 40017;
const IDM_FIND: u16 = 40018;
const IDM_FINDNEXT: u16 = 40019; // grayed
const IDM_HIDECATS: u16 = 40030;
const IDM_SORT_ORDER: u16 = 40032;
const IDM_SORT_NAME: u16 = 40033;
const IDM_SORT_SIZE: u16 = 40034;
const IDM_SORT_STATUS: u16 = 40035;
const IDM_SORT_TIMELEFT: u16 = 40036;
const IDM_SORT_RATE: u16 = 40037;
const IDM_ABOUT: u16 = 40060;

// Toolbar-only IDs for buttons that have no menu equivalent.
const IDM_TB_OPTIONS: u16 = 40070; // disabled
const IDM_TB_SCHEDULER: u16 = 40071; // disabled
const IDM_TB_QUEUE: u16 = 40072; // disabled
const IDM_TB_LIMITER: u16 = 40073; // disabled

// App → poller message.
const WM_APP_REFRESH: u32 = WM_APP + 1;

// Window classes.
const CLASS_MAIN: PCWSTR = w!("DmMainWindow");
const CLASS_PROGRESS: PCWSTR = w!("DmProgressWnd");
const CLASS_ADDURL: PCWSTR = w!("DmAddUrl");
const CLASS_STATS: PCWSTR = w!("DmStatsWnd");

const CAT_NAMES: [&str; 6] = ["Default", "Video", "Programs", "Music", "Documents", "Compressed"];
const TREE_PANE_W: i32 = 196;
const REBAR_H: i32 = 32;

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Controls {
    list: HWND,
    tree: HWND,
    status: HWND,
    search: HWND,
    cathead: HWND,
    rebar: HWND,
    toolbar: HWND,
}

// Window handles are opaque integers at runtime, only ever touched on the UI
// thread, so caching them in statics is safe.
unsafe impl Send for Controls {}
unsafe impl Sync for Controls {}

static CTRL: Mutex<Option<Controls>> = Mutex::new(None);
static STATS_LIST: Mutex<Option<isize>> = Mutex::new(None);

#[derive(Clone, Default)]
struct Snapshot {
    ok: bool,
    downloads: Vec<Download>,
    daily: Vec<DailyStat>,
}

static SNAPSHOT: Mutex<Option<Snapshot>> = Mutex::new(None);
static QUIT: AtomicBool = AtomicBool::new(false);

// Open progress windows: (raw HWND, download id).
static PROGRESS_WINS: Mutex<Vec<(isize, String)>> = Mutex::new(Vec::new());
// Statistics window raw HWND.
static STATS_HWND: Mutex<Option<isize>> = Mutex::new(None);
// Tree item handles (raw HTREEITEM) in CAT_NAMES order.
static TREE_ITEMS: Mutex<Vec<isize>> = Mutex::new(Vec::new());
// Displayed list rows: id per row (display order) + custom-draw text colors.
static ROW_IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static ROW_COLORS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
// Search filter text (lowercase) and view state.
static SEARCH: Mutex<String> = Mutex::new(String::new());
static SORT: AtomicU8 = AtomicU8::new(0);
static CAT_VISIBLE: AtomicBool = AtomicBool::new(true);
static CAT_SEL: AtomicUsize = AtomicUsize::new(0);
// Raw HMENU handles for checkmarks.
static VIEW_SUB: Mutex<Option<isize>> = Mutex::new(None);
static ARRANGE_SUB: Mutex<Option<isize>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES
                | ICC_TREEVIEW_CLASSES
                | ICC_BAR_CLASSES
                | ICC_PROGRESS_CLASS
                | ICC_COOL_CLASSES,
        });

        let icon = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(main_wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hIcon: icon,
            hbrBackground: HBRUSH(((COLOR_WINDOW.0 + 1) as isize) as *mut _),
            lpszClassName: CLASS_MAIN,
            ..Default::default()
        };
        RegisterClassW(&wc);
        for (cls, proc) in [
            (CLASS_PROGRESS, Some(progress_wnd_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT)),
            (CLASS_ADDURL, Some(addurl_wnd_proc)),
            (CLASS_STATS, Some(stats_wnd_proc)),
        ] {
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: proc,
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hIcon: icon,
                hbrBackground: HBRUSH(((COLOR_WINDOW.0 + 1) as isize) as *mut _),
                lpszClassName: cls,
                ..Default::default()
            };
            RegisterClassW(&wc);
        }

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_MAIN,
            w!("Download Manager"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1040,
            700,
            HWND::default(),
            HMENU::default(),
            instance,
            None,
        )?;

        // Poller thread: fetch from the service, wake the UI thread.
        // HWND isn't Send, so hand the thread the raw handle as an isize.
        let poll_hwnd_raw = hwnd.0 as isize;
        std::thread::spawn(move || poller(HWND(poll_hwnd_raw as *mut _)));

        let accel = CreateAcceleratorTableW(&[
            ACCEL {
                fVirt: ACCEL_VIRT_FLAGS(FVIRTKEY.0 | FCONTROL.0),
                key: b'N' as u16,
                cmd: IDM_ADDURL,
            },
            ACCEL {
                fVirt: ACCEL_VIRT_FLAGS(FVIRTKEY.0 | FCONTROL.0),
                key: b'F' as u16,
                cmd: IDM_FIND,
            },
        ])?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if TranslateAcceleratorW(hwnd, accel, &msg) != 0 {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        QUIT.store(true, Ordering::SeqCst);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Main window
// ---------------------------------------------------------------------------

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_controls(hwnd);
            layout(hwnd);
            let _ = SetMenu(hwnd, build_menu());
            LRESULT(0)
        }
        WM_SIZE => {
            layout(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u16;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            on_command(hwnd, id, code);
            LRESULT(0)
        }
        WM_NOTIFY => {
            let Some(c) = *CTRL.lock().unwrap() else {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            };
            let hdr = &*(lparam.0 as *const NMHDR);
            if hdr.hwndFrom == c.tree {
                if hdr.code == TVN_SELCHANGEDW {
                    let nmtv = &*(lparam.0 as *const NMTREEVIEWW);
                    let h = nmtv.itemNew.hItem.0 as isize;
                    let idx = TREE_ITEMS.lock().unwrap().iter().position(|&x| x == h);
                    if let Some(i) = idx {
                        CAT_SEL.store(i, Ordering::SeqCst);
                        refresh();
                    }
                    return LRESULT(0);
                }
            } else if hdr.hwndFrom == c.list {
                if hdr.code == NM_DBLCLK {
                    open_progress_for_selection(hwnd);
                    return LRESULT(0);
                }
                // Status (col 2) and Transfer rate (col 4) get colored text.
                if hdr.code == NM_CUSTOMDRAW || hdr.code == 4294967184 {
                    return list_custom_draw(lparam);
                }
            }
            LRESULT(0)
        }
        WM_APP_REFRESH => {
            refresh();
            LRESULT(0)
        }
        WM_DESTROY => {
            QUIT.store(true, Ordering::SeqCst);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn create_controls(hwnd: HWND) {
    let instance = GetModuleHandleW(None).unwrap_or_default();
    let font = HGDIOBJ(GetStockObject(DEFAULT_GUI_FONT).0);

    // --- ReBar + flat toolbar with GDI-drawn icons ---
    let rebar = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("ReBarWindow32"),
        PCWSTR::null(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
        0,
        0,
        0,
        0,
        hwnd,
        HMENU::default(),
        instance,
        None,
    )
    .unwrap_or_default();

    let toolbar = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("ToolbarWindow32"),
        PCWSTR::null(),
        WINDOW_STYLE((WS_CHILD.0 | WS_VISIBLE.0 | TBSTYLE_FLAT | TBSTYLE_TOOLTIPS) as u32),
        0,
        0,
        0,
        0,
        rebar,
        HMENU::default(),
        instance,
        None,
    )
    .unwrap_or_default();

    SendMessageW(
        toolbar,
        TB_BUTTONSTRUCTSIZE,
        WPARAM(std::mem::size_of::<TBBUTTON>()),
        LPARAM(0),
    );
    SendMessageW(
        toolbar,
        TB_SETBUTTONSIZE,
        WPARAM(0),
        LPARAM(((24u32 << 16) | 24u32) as isize),
    );
    let himl = make_toolbar_imagelist();
    SendMessageW(
        toolbar,
        TB_SETIMAGELIST,
        WPARAM(0),
        LPARAM(himl.0 as isize),
    );
    SendMessageW(
        toolbar,
        TB_SETEXTENDEDSTYLE,
        WPARAM(0),
        LPARAM(TBSTYLE_EX_DOUBLEBUFFER as isize),
    );

    // Buttons: icon index, command id, enabled?, tooltip. Separators between
    // groups, mirroring IDM's toolbar layout.
    let tip = |s: &'static str| -> isize { Box::leak(wide(s).into_boxed_slice()).as_ptr() as isize };
    let btn = |icon: i32, id: u16, enabled: bool, t: &'static str| TBBUTTON {
        iBitmap: icon,
        idCommand: id as i32,
        fsState: if enabled { TBSTATE_ENABLED as u8 } else { 0 },
        fsStyle: BTNS_BUTTON as u8,
        bReserved: [0; 6],
        dwData: 0,
        iString: tip(t),
    };
    let sep = || TBBUTTON {
        iBitmap: -1,
        idCommand: 0,
        fsState: 0,
        fsStyle: BTNS_SEP as u8,
        bReserved: [0; 6],
        dwData: 0,
        iString: 0,
    };
    let buttons = [
        btn(0, IDM_ADDURL, true, "Add new download…"),
        sep(),
        btn(1, IDM_DOWNLOADNOW, true, "Resume download"),
        btn(2, IDM_STOPDL, true, "Stop download"),
        btn(3, IDM_STOPALL, true, "Stop all downloads"),
        sep(),
        btn(4, IDM_REMOVESEL, true, "Remove download"),
        btn(5, IDM_TB_OPTIONS, false, "Options"),
        btn(6, IDM_TB_SCHEDULER, false, "Scheduler"),
        sep(),
        btn(7, IDM_TB_QUEUE, false, "Queue"),
        btn(8, IDM_TB_LIMITER, false, "Speed limiter"),
    ];
    SendMessageW(
        toolbar,
        TB_ADDBUTTONSW,
        WPARAM(buttons.len()),
        LPARAM(buttons.as_ptr() as isize),
    );
    SendMessageW(toolbar, TB_AUTOSIZE, WPARAM(0), LPARAM(0));

    // One band holding the toolbar, spanning the full width (no gripper, like
    // IDM's single toolbar row).
    let rbi = REBARBANDINFOW {
        cbSize: std::mem::size_of::<REBARBANDINFOW>() as u32,
        fMask: RBBIM_STYLE | RBBIM_CHILD | RBBIM_CHILDSIZE | RBBIM_SIZE,
        fStyle: RBBS_CHILDEDGE,
        hwndChild: toolbar,
        cxMinChild: 200,
        cyMinChild: 30,
        cyChild: 30,
        cx: 4000,
        ..Default::default()
    };
    SendMessageW(
        rebar,
        RB_INSERTBANDW,
        WPARAM(0),
        LPARAM(&rbi as *const REBARBANDINFOW as isize),
    );

    // --- Search box (floats over the right end of the toolbar row) ---
    let search = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("EDIT"),
        PCWSTR::null(),
        WINDOW_STYLE((WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | ES_AUTOHSCROLL as u32) as u32),
        0,
        0,
        160,
        22,
        hwnd,
        HMENU(IDC_SEARCH as usize as *mut _),
        instance,
        None,
    )
    .unwrap_or_default();
    let cue = wide("Search downloads");
    SendMessageW(
        search,
        EM_SETCUEBANNER,
        WPARAM(1),
        LPARAM(cue.as_ptr() as isize),
    );
    SendMessageW(search, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));

    // --- Category header + tree ---
    let cathead = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("STATIC"),
        w!("Category"),
        WINDOW_STYLE((WS_CHILD.0 | WS_VISIBLE.0) as u32),
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(IDC_CATHEAD as usize as *mut _),
        instance,
        None,
    )
    .unwrap_or_default();
    SendMessageW(cathead, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));

    let tree = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("SysTreeView32"),
        PCWSTR::null(),
        WINDOW_STYLE(
            (WS_CHILD.0
                | WS_VISIBLE.0
                | TVS_HASBUTTONS
                | TVS_HASLINES
                | TVS_LINESATROOT
                | TVS_SHOWSELALWAYS) as u32,
        ),
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(IDC_TREE as usize as *mut _),
        instance,
        None,
    )
    .unwrap_or_default();
    SendMessageW(tree, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    // Light left-pane background like the mock.
    SendMessageW(
        tree,
        TVM_SETBKCOLOR,
        WPARAM(0),
        LPARAM(COLORREF(0x00FAF7F4).0 as isize),
    );
    SendMessageW(
        tree,
        TVM_SETTEXTCOLOR,
        WPARAM(0),
        LPARAM(COLORREF(0x00382A1B).0 as isize),
    );
    let tree_himl = make_tree_imagelist();
    SendMessageW(
        tree,
        TVM_SETIMAGELIST,
        WPARAM(TVSIL_NORMAL as usize),
        LPARAM(tree_himl.0 as isize),
    );
    for (i, name) in CAT_NAMES.iter().enumerate() {
        let mut label = wide(&format!("{name} (0)"));
        let item = TVITEMW {
            mask: TVIF_TEXT | TVIF_IMAGE | TVIF_SELECTEDIMAGE,
            iImage: i as i32,
            iSelectedImage: i as i32,
            pszText: PWSTR(label.as_mut_ptr()),
            ..Default::default()
        };
        let ins = TVINSERTSTRUCTW {
            hParent: TVI_ROOT,
            hInsertAfter: TVI_LAST,
            Anonymous: TVINSERTSTRUCTW_0 { item },
        };
        let res = SendMessageW(
            tree,
            TVM_INSERTITEMW,
            WPARAM(0),
            LPARAM(&ins as *const TVINSERTSTRUCTW as isize),
        );
        TREE_ITEMS.lock().unwrap().push(res.0);
    }

    // --- File list (7 IDM columns) ---
    let list = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("SysListView32"),
        PCWSTR::null(),
        WINDOW_STYLE(
            (WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | LVS_REPORT | LVS_SHOWSELALWAYS) as u32,
        ),
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(IDC_LIST as usize as *mut _),
        instance,
        None,
    )
    .unwrap_or_default();
    SendMessageW(list, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    SendMessageW(
        list,
        LVM_SETEXTENDEDLISTVIEWSTYLE,
        WPARAM(0),
        LPARAM((LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_GRIDLINES) as isize),
    );
    const COLS: [(&str, i32); 7] = [
        ("File Name", 280),
        ("Size", 76),
        ("Status", 130),
        ("Downloaded", 84),
        ("Transfer rate", 94),
        ("Time left", 76),
        ("Resume capability", 140),
    ];
    for (i, (title, wdt)) in COLS.iter().enumerate() {
        insert_column(list, i as i32, title, *wdt);
    }

    // --- Status bar (4 parts) ---
    let status = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("msctls_statusbar32"),
        PCWSTR::null(),
        WINDOW_STYLE((WS_CHILD.0 | WS_VISIBLE.0 | SBARS_SIZEGRIP) as u32),
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(IDC_STATUS as usize as *mut _),
        instance,
        None,
    )
    .unwrap_or_default();
    SendMessageW(status, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    let parts: [i32; 4] = [180, 430, 660, -1];
    SendMessageW(
        status,
        SB_SETPARTS,
        WPARAM(parts.len()),
        LPARAM(parts.as_ptr() as isize),
    );

    *CTRL.lock().unwrap() = Some(Controls {
        list,
        tree,
        status,
        search,
        cathead,
        rebar,
        toolbar,
    });
}

unsafe fn layout(hwnd: HWND) {
    let Some(c) = *CTRL.lock().unwrap() else { return };
    let mut rc = RECT::default();
    if GetClientRect(hwnd, &mut rc).is_err() {
        return;
    }
    let cw = rc.right - rc.left;
    let ch = rc.bottom - rc.top;

    MoveWindow(c.rebar, 0, 0, cw, REBAR_H, BOOL::from(true));
    SendMessageW(c.toolbar, TB_AUTOSIZE, WPARAM(0), LPARAM(0));
    // Search box floats at the right end of the toolbar row.
    MoveWindow(c.search, cw - 172, 5, 162, 22, BOOL::from(true));

    let sb_h = 23;
    MoveWindow(c.status, 0, ch - sb_h, cw, sb_h, BOOL::from(true));

    let cat_visible = CAT_VISIBLE.load(Ordering::SeqCst);
    let cat_w = if cat_visible { TREE_PANE_W } else { 0 };
    let body_h = ch - REBAR_H - sb_h;
    MoveWindow(c.cathead, 0, REBAR_H, cat_w, 22, BOOL::from(true));
    MoveWindow(c.tree, 0, REBAR_H + 22, cat_w, body_h - 22, BOOL::from(true));
    MoveWindow(
        c.list,
        cat_w,
        REBAR_H,
        (cw - cat_w).max(0),
        body_h,
        BOOL::from(true),
    );
    // Last column takes the remaining width.
    let used: i32 = 280 + 76 + 130 + 84 + 94 + 76;
    let last = (cw - cat_w - used - 20).max(60);
    SendMessageW(
        c.list,
        LVM_SETCOLUMNWIDTH,
        WPARAM(6),
        LPARAM(last as isize),
    );
}

// ---------------------------------------------------------------------------
// Menus
// ---------------------------------------------------------------------------

unsafe fn mitem(m: HMENU, id: u16, text: &str, flags: MENU_ITEM_FLAGS) {
    let t = wide(text);
    let _ = AppendMenuW(m, flags, id as usize, PCWSTR(t.as_ptr()));
}

unsafe fn build_menu() -> HMENU {
    let bar = CreateMenu().unwrap_or_default();

    // File
    let file = CreatePopupMenu().unwrap_or_default();
    mitem(file, IDM_ADDURL, "Add new download…\tCtrl+N", MF_STRING);
    mitem(
        file,
        0,
        "Add batch download…",
        MF_STRING | MF_GRAYED,
    );
    mitem(
        file,
        0,
        "Add batch download from clipboard",
        MF_STRING | MF_GRAYED,
    );
    mitem(file, 0, "Run site grabber", MF_STRING | MF_GRAYED);
    mitem(file, 0, "", MF_SEPARATOR);
    mitem(file, 0, "Show drop target", MF_STRING | MF_GRAYED);
    mitem(file, 0, "", MF_SEPARATOR);
    let export = CreatePopupMenu().unwrap_or_default();
    mitem(export, 0, "To IDM export file", MF_STRING | MF_GRAYED);
    mitem(export, 0, "To text file", MF_STRING | MF_GRAYED);
    let _ = AppendMenuW(file, MF_POPUP, export.0 as usize, PCWSTR(wide("Export").as_ptr()));
    mitem(file, 0, "", MF_SEPARATOR);
    mitem(file, IDM_EXIT, "Exit\tAlt+F4", MF_STRING);
    let _ = AppendMenuW(bar, MF_POPUP, file.0 as usize, PCWSTR(wide("File").as_ptr()));

    // Downloads
    let dl = CreatePopupMenu().unwrap_or_default();
    mitem(dl, IDM_DOWNLOADNOW, "Download Now\tResume", MF_STRING);
    mitem(dl, IDM_REDOWNLOAD, "Redownload", MF_STRING);
    mitem(dl, IDM_STOPDL, "Stop Download", MF_STRING);
    mitem(dl, IDM_PAUSEALL, "Pause All", MF_STRING | MF_GRAYED);
    mitem(dl, IDM_STOPALL, "Stop All", MF_STRING);
    mitem(dl, IDM_DELETECOMPLETED, "Delete All Completed", MF_STRING);
    mitem(dl, IDM_REMOVESEL, "Remove", MF_STRING);
    mitem(dl, 0, "", MF_SEPARATOR);
    mitem(dl, IDM_STATISTICS, "Statistics…", MF_STRING);
    mitem(dl, 0, "", MF_SEPARATOR);
    mitem(dl, IDM_FIND, "Find…\tCtrl+F", MF_STRING);
    mitem(dl, IDM_FINDNEXT, "Find Next\tF3", MF_STRING | MF_GRAYED);
    mitem(dl, 0, "", MF_SEPARATOR);
    let limiter = CreatePopupMenu().unwrap_or_default();
    mitem(limiter, 0, "Enabled", MF_STRING | MF_GRAYED);
    mitem(limiter, 0, "Set maximum download rate…", MF_STRING | MF_GRAYED);
    let _ = AppendMenuW(
        dl,
        MF_POPUP,
        limiter.0 as usize,
        PCWSTR(wide("Speed Limiter").as_ptr()),
    );
    let _ = AppendMenuW(bar, MF_POPUP, dl.0 as usize, PCWSTR(wide("Downloads").as_ptr()));

    // Scheduler
    let sched = CreatePopupMenu().unwrap_or_default();
    mitem(sched, 0, "Start queue", MF_STRING | MF_GRAYED);
    mitem(sched, 0, "Stop queue", MF_STRING | MF_GRAYED);
    mitem(sched, 0, "", MF_SEPARATOR);
    mitem(sched, 0, "Scheduler…", MF_STRING | MF_GRAYED);
    let _ = AppendMenuW(
        bar,
        MF_POPUP,
        sched.0 as usize,
        PCWSTR(wide("Scheduler").as_ptr()),
    );

    // Options
    let opt = CreatePopupMenu().unwrap_or_default();
    mitem(opt, 0, "Options…", MF_STRING | MF_GRAYED);
    mitem(opt, 0, "Grabber Settings", MF_STRING | MF_GRAYED);
    mitem(opt, 0, "Speed Limiter", MF_STRING | MF_GRAYED);
    let _ = AppendMenuW(
        bar,
        MF_POPUP,
        opt.0 as usize,
        PCWSTR(wide("Options").as_ptr()),
    );

    // View
    let view = CreatePopupMenu().unwrap_or_default();
    mitem(view, IDM_HIDECATS, "Hide categories", MF_STRING);
    mitem(view, 0, "", MF_SEPARATOR);
    let arrange = CreatePopupMenu().unwrap_or_default();
    mitem(arrange, IDM_SORT_ORDER, "By order of addition", MF_STRING);
    mitem(arrange, IDM_SORT_NAME, "By file name", MF_STRING);
    mitem(arrange, IDM_SORT_SIZE, "By size", MF_STRING);
    mitem(arrange, IDM_SORT_STATUS, "By status", MF_STRING);
    mitem(arrange, IDM_SORT_TIMELEFT, "By time left", MF_STRING);
    mitem(arrange, IDM_SORT_RATE, "By transfer rate", MF_STRING);
    mitem(arrange, 0, "", MF_SEPARATOR);
    mitem(arrange, 0, "By last try date", MF_STRING | MF_GRAYED);
    mitem(arrange, 0, "By description", MF_STRING | MF_GRAYED);
    mitem(arrange, 0, "By save path", MF_STRING | MF_GRAYED);
    mitem(arrange, 0, "By referer", MF_STRING | MF_GRAYED);
    mitem(arrange, 0, "By parent page", MF_STRING | MF_GRAYED);
    let _ = AppendMenuW(
        view,
        MF_POPUP,
        arrange.0 as usize,
        PCWSTR(wide("Arrange files").as_ptr()),
    );
    mitem(view, 0, "", MF_SEPARATOR);
    let tbm = CreatePopupMenu().unwrap_or_default();
    mitem(tbm, 0, "Show text labels", MF_STRING | MF_GRAYED);
    mitem(tbm, 0, "Show large buttons", MF_STRING | MF_GRAYED);
    mitem(tbm, 0, "Show small buttons", MF_STRING | MF_GRAYED);
    let _ = AppendMenuW(
        view,
        MF_POPUP,
        tbm.0 as usize,
        PCWSTR(wide("Toolbar").as_ptr()),
    );
    mitem(view, 0, "Dark Mode support", MF_STRING | MF_GRAYED);
    mitem(view, 0, "Font…", MF_STRING | MF_GRAYED);
    let _ = AppendMenuW(bar, MF_POPUP, view.0 as usize, PCWSTR(wide("View").as_ptr()));
    *VIEW_SUB.lock().unwrap() = Some(view.0 as isize);
    *ARRANGE_SUB.lock().unwrap() = Some(arrange.0 as isize);
    // Initial radio check on the sort menu.
    let _ = CheckMenuRadioItem(
        arrange,
        IDM_SORT_ORDER as u32,
        IDM_SORT_RATE as u32,
        IDM_SORT_ORDER as u32,
        MF_BYCOMMAND.0,
    );

    // Language
    let lang = CreatePopupMenu().unwrap_or_default();
    mitem(lang, 0, "English", MF_STRING | MF_CHECKED);
    for l in [
        "Chinese", "Dutch", "French", "German", "Greek", "Italian", "Japanese", "Korean",
        "Polish", "Portuguese", "Russian", "Spanish", "Turkish",
    ] {
        mitem(lang, 0, l, MF_STRING | MF_GRAYED);
    }
    let _ = AppendMenuW(
        bar,
        MF_POPUP,
        lang.0 as usize,
        PCWSTR(wide("Language").as_ptr()),
    );

    // Help
    let help = CreatePopupMenu().unwrap_or_default();
    mitem(help, 0, "Download Manager Help", MF_STRING | MF_GRAYED);
    mitem(help, 0, "Tutorials", MF_STRING | MF_GRAYED);
    mitem(help, 0, "Tip of the Day", MF_STRING | MF_GRAYED);
    mitem(help, 0, "", MF_SEPARATOR);
    mitem(help, 0, "Check for Updates", MF_STRING | MF_GRAYED);
    mitem(help, IDM_ABOUT, "About Download Manager…", MF_STRING);
    let _ = AppendMenuW(bar, MF_POPUP, help.0 as usize, PCWSTR(wide("Help").as_ptr()));

    bar
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

unsafe fn on_command(hwnd: HWND, id: u16, code: u32) {
    match id {
        IDC_SEARCH if code == EN_CHANGE => {
            *SEARCH.lock().unwrap() = get_text_of(CTRL.lock().unwrap().unwrap().search).to_lowercase();
            refresh();
        }
        IDM_ADDURL => show_add_url(hwnd),
        IDM_EXIT => {
            let _ = DestroyWindow(hwnd);
        }
        IDM_DOWNLOADNOW | IDM_REDOWNLOAD => restart_selected(),
        IDM_STOPDL => {
            if let Some(id) = selected_download_id() {
                std::thread::spawn(move || {
                    let _ = api::cancel_download(&id);
                });
            }
        }
        IDM_STOPALL => {
            let ids: Vec<String> = SNAPSHOT
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default()
                .downloads
                .into_iter()
                .filter(|d| d.status == "running")
                .map(|d| d.id)
                .collect();
            std::thread::spawn(move || {
                for id in ids {
                    let _ = api::cancel_download(&id);
                }
            });
        }
        IDM_DELETECOMPLETED => {
            let ids: Vec<String> = SNAPSHOT
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default()
                .downloads
                .into_iter()
                .filter(|d| d.status == "completed")
                .map(|d| d.id)
                .collect();
            std::thread::spawn(move || {
                for id in ids {
                    let _ = api::delete_download(&id);
                }
            });
        }
        IDM_REMOVESEL => {
            if let Some(id) = selected_download_id() {
                close_progress_for(&id);
                std::thread::spawn(move || {
                    let _ = api::delete_download(&id);
                });
            }
        }
        IDM_STATISTICS => open_stats(hwnd),
        IDM_FIND => {
            if let Some(c) = *CTRL.lock().unwrap() {
                let _ = SetFocus(c.search);
                SendMessageW(c.search, EM_SETSEL, WPARAM(0), LPARAM(-1));
            }
        }
        IDM_HIDECATS => {
            let now = !CAT_VISIBLE.load(Ordering::SeqCst);
            CAT_VISIBLE.store(now, Ordering::SeqCst);
            if let (Some(view), Some(arr)) =
                (*VIEW_SUB.lock().unwrap(), *ARRANGE_SUB.lock().unwrap())
            {
                let _ = CheckMenuItem(
                    HMENU(view as *mut _),
                    IDM_HIDECATS as u32,
                    if now { 0u32 } else { 8u32 }, // MF_UNCHECKED / MF_CHECKED
                );
                let _ = arr; // arrange handle kept for sort radio checks
            }
            if let Some(c) = *CTRL.lock().unwrap() {
                let cmd = if now { SW_HIDE } else { SW_SHOW };
                ShowWindow(c.tree, cmd);
                ShowWindow(c.cathead, cmd);
            }
            layout(hwnd);
            refresh();
        }
        IDM_SORT_ORDER..=IDM_SORT_RATE => {
            SORT.store((id - IDM_SORT_ORDER) as u8, Ordering::SeqCst);
            if let Some(arr) = *ARRANGE_SUB.lock().unwrap() {
                let _ = CheckMenuRadioItem(
                    HMENU(arr as *mut _),
                    IDM_SORT_ORDER as u32,
                    IDM_SORT_RATE as u32,
                    id as u32,
                    MF_BYCOMMAND.0,
                );
            }
            refresh();
        }
        IDM_ABOUT => {
            MessageBoxW(
                hwnd,
                w!("Download Manager 1.0\r\n\r\nA tiny native Win32 download manager with multi-segment acceleration, a Windows service core, and browser integration."),
                w!("About Download Manager"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        IDM_TB_OPTIONS | IDM_TB_SCHEDULER | IDM_TB_QUEUE | IDM_TB_LIMITER => {
            // Toolbar placeholders — disabled at creation; nothing to do.
        }
        _ => {}
    }
}

/// Resume/Redownload: re-POST the same URL (service has no pause/resume), and
/// drop the stale record so the list doesn't hold duplicates.
fn restart_selected() {
    let Some(id) = selected_download_id() else { return };
    let dl = SNAPSHOT
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
        .downloads
        .into_iter()
        .find(|d| d.id == id);
    if let Some(d) = dl {
        std::thread::spawn(move || {
            let _ = api::delete_download(&d.id);
            let _ = api::add_download(&d.url, d.segments.max(1));
        });
    }
}

fn selected_download_id() -> Option<String> {
    let c = (*CTRL.lock().unwrap())?;
    let list = c.list;
    let row = unsafe {
        SendMessageW(
            list,
            LVM_GETNEXTITEM,
            WPARAM(usize::MAX),
            LPARAM(LVNI_SELECTED as isize),
        )
        .0 as i32
    };
    if row < 0 {
        return None;
    }
    ROW_IDS.lock().unwrap().get(row as usize).cloned()
}

// ---------------------------------------------------------------------------
// Add-URL popup (modeless, run modally via a nested message loop)
// ---------------------------------------------------------------------------

unsafe fn show_add_url(owner: HWND) {
    let instance = GetModuleHandleW(None).unwrap_or_default();

    // Center over the main window.
    let mut orc = RECT::default();
    let _ = GetWindowRect(owner, &mut orc);
    let (w, h) = (480, 236);
    let x = orc.left + ((orc.right - orc.left) - w) / 2;
    let y = orc.top + ((orc.bottom - orc.top) - h) / 2;

    let Ok(hwnd) = CreateWindowExW(
        WS_EX_DLGMODALFRAME,
        CLASS_ADDURL,
        w!("Add new download"),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        x,
        y,
        w,
        h,
        owner,
        HMENU::default(),
        instance,
        None,
    ) else {
        return;
    };

    let _ = EnableWindow(owner, BOOL::from(false));
    let _ = SetForegroundWindow(hwnd);

    let mut msg = MSG::default();
    while IsWindow(hwnd).as_bool() {
        if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
            break;
        }
        if !IsDialogMessageW(hwnd, &msg).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    let _ = EnableWindow(owner, BOOL::from(true));
    let _ = SetForegroundWindow(owner);
}

unsafe extern "system" fn addurl_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let font = GetStockObject(DEFAULT_GUI_FONT);
            let mk = |cls: PCWSTR,
                      text: PCWSTR,
                      style: u32,
                      id: u16,
                      x: i32,
                      y: i32,
                      w: i32,
                      h: i32|
             -> HWND {
                let hwn = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    cls,
                    text,
                    WINDOW_STYLE(style),
                    x,
                    y,
                    w,
                    h,
                    hwnd,
                    HMENU(id as usize as *mut _),
                    instance,
                    None,
                )
                .unwrap_or_default();
                SendMessageW(hwn, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                hwn
            };
            mk(w!("STATIC"), w!("Address (URL):"), WS_CHILD.0 | WS_VISIBLE.0, 0, 12, 12, 300, 16);
            mk(
                w!("EDIT"),
                PCWSTR::null(),
                (WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | WS_TABSTOP.0 | ES_AUTOHSCROLL as u32) as u32,
                IDC_URL,
                12,
                30,
                440,
                22,
            );
            mk(
                w!("STATIC"),
                w!("Number of connections (1-64):"),
                WS_CHILD.0 | WS_VISIBLE.0,
                0,
                12,
                62,
                300,
                16,
            );
            let segs = mk(
                w!("EDIT"),
                w!("8"),
                (WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | WS_TABSTOP.0 | ES_NUMBER as u32) as u32,
                IDC_SEGS,
                12,
                80,
                60,
                22,
            );
            mk(
                w!("BUTTON"),
                w!("Download"),
                (WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_DEFPUSHBUTTON as u32) as u32,
                1u16,
                248,
                158,
                95,
                26,
            );
            mk(
                w!("BUTTON"),
                w!("Cancel"),
                (WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0) as u32,
                2u16,
                351,
                158,
                95,
                26,
            );
            let _ = SetFocus(segs);
            let _ = SetFocus(GetDlgItem(hwnd, IDC_URL as i32).unwrap_or_default());
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u16;
            match id {
                1u16 => {
                    let url = get_text(GetDlgItem(hwnd, IDC_URL as i32).unwrap_or_default());
                    let segs: u32 = get_text(GetDlgItem(hwnd, IDC_SEGS as i32).unwrap_or_default())
                        .parse()
                        .unwrap_or(8)
                        .clamp(1, 64);
                    if !url.is_empty() {
                        std::thread::spawn(move || {
                            let _ = api::add_download(&url, segs);
                        });
                    }
                    let _ = DestroyWindow(hwnd);
                }
                2u16 => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ---------------------------------------------------------------------------
// Per-download progress windows
// ---------------------------------------------------------------------------

unsafe fn open_progress_for_selection(owner: HWND) {
    let Some(id) = selected_download_id() else { return };
    let Some(d) = SNAPSHOT
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
        .downloads
        .into_iter()
        .find(|d| d.id == id)
    else {
        return;
    };
    // Already open? Bring to front.
    let existing = PROGRESS_WINS
        .lock()
        .unwrap()
        .iter()
        .find(|(_, did)| *did == id)
        .map(|(h, _)| *h);
    if let Some(h) = existing {
        let _ = SetForegroundWindow(HWND(h as *mut _));
        return;
    }
    let instance = GetModuleHandleW(None).unwrap_or_default();
    let title = wide(&d.filename);
    if let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        CLASS_PROGRESS,
        PCWSTR(title.as_ptr()),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        452,
        306,
        owner,
        HMENU::default(),
        instance,
        None,
    ) {
        PROGRESS_WINS.lock().unwrap().push((hwnd.0 as isize, id.clone()));
        update_progress_window(hwnd, &id);
    }
}

unsafe fn update_progress_window(hwnd: HWND, id: &str) {
    let snap = SNAPSHOT.lock().unwrap().clone().unwrap_or_default();
    let d = snap.downloads.iter().find(|d| d.id == id);
    let set = |ctl: i32, text: &str| {
        let t = wide(text);
        SetWindowTextW(GetDlgItem(hwnd, ctl).unwrap_or_default(), PCWSTR(t.as_ptr()));
    };
    let Some(d) = d else {
        set(IDC_P_STATUS as i32, "Finished or removed");
        EnableWindow(GetDlgItem(hwnd, IDC_P_STOP as i32).unwrap_or_default(), BOOL::from(false));
        EnableWindow(GetDlgItem(hwnd, IDC_P_REMOVE as i32).unwrap_or_default(), BOOL::from(false));
        return;
    };
    let pct = if d.total_bytes > 0 {
        ((d.downloaded_bytes.min(d.total_bytes)) * 1000 / d.total_bytes) as isize
    } else if d.status == "completed" {
        1000
    } else {
        0
    };
    SendMessageW(
        GetDlgItem(hwnd, IDC_P_BAR as i32).unwrap_or_default(),
        PBM_SETPOS,
        WPARAM(pct as usize),
        LPARAM(0),
    );
    let dl = if d.total_bytes > 0 {
        format!(
            "{} of {} ({}%)",
            fmt_bytes(d.downloaded_bytes),
            fmt_bytes(d.total_bytes),
            pct / 10
        )
    } else {
        fmt_bytes(d.downloaded_bytes)
    };
    set(IDC_P_DOWN as i32, &dl);
    let rate = if d.status == "running" {
        fmt_bps(d.avg_bps)
    } else {
        "-".into()
    };
    set(IDC_P_RATE as i32, &rate);
    let eta = if d.status == "running" {
        fmt_time(eta_secs(d))
    } else {
        "-".into()
    };
    set(IDC_P_TIME as i32, &eta);
    set(IDC_P_STATUS as i32, &status_text(d));
    set(
        IDC_P_SEG as i32,
        &format!("{} connection{}", d.segments.max(1), if d.segments == 1 { "" } else { "s" }),
    );
    EnableWindow(
        GetDlgItem(hwnd, IDC_P_STOP as i32).unwrap_or_default(),
        BOOL::from(d.status == "running"),
    );
}

unsafe extern "system" fn progress_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let font = GetStockObject(DEFAULT_GUI_FONT);
            let bold = {
                let mut lf = LOGFONTW::default();
                lf.lfHeight = -13;
                lf.lfWeight = FW_BOLD.0 as i32;
                let face = wide("Segoe UI");
                for (i, c) in face.iter().take(31).enumerate() {
                    lf.lfFaceName[i] = *c;
                }
                CreateFontIndirectW(&lf)
            };
            let mk = |cls: PCWSTR,
                      text: PCWSTR,
                      style: u32,
                      id: u16,
                      x: i32,
                      y: i32,
                      w: i32,
                      h: i32|
             -> HWND {
                let hwn = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    cls,
                    text,
                    WINDOW_STYLE(style),
                    x,
                    y,
                    w,
                    h,
                    hwnd,
                    HMENU(id as usize as *mut _),
                    instance,
                    None,
                )
                .unwrap_or_default();
                SendMessageW(hwn, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                hwn
            };
            let f = WS_CHILD.0 | WS_VISIBLE.0;
            let file = mk(w!("STATIC"), w!("file"), f, IDC_P_FILE, 12, 12, 410, 18);
            SendMessageW(file, WM_SETFONT, WPARAM(bold.0 as usize), LPARAM(1));
            mk(w!("STATIC"), w!("Downloaded:"), f, 0, 12, 42, 92, 16);
            mk(w!("STATIC"), w!("-"), f, IDC_P_DOWN, 108, 42, 320, 16);
            let bar = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("msctls_progress32"),
                PCWSTR::null(),
                WINDOW_STYLE((f | PBS_SMOOTH) as u32),
                12,
                64,
                410,
                16,
                hwnd,
                HMENU(IDC_P_BAR as usize as *mut _),
                instance,
                None,
            )
            .unwrap_or_default();
            SendMessageW(bar, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            SendMessageW(bar, PBM_SETRANGE32, WPARAM(0), LPARAM(1000));
            mk(w!("STATIC"), w!("Transfer rate:"), f, 0, 12, 92, 92, 16);
            mk(w!("STATIC"), w!("-"), f, IDC_P_RATE, 108, 92, 320, 16);
            mk(w!("STATIC"), w!("Time left:"), f, 0, 12, 112, 92, 16);
            mk(w!("STATIC"), w!("-"), f, IDC_P_TIME, 108, 112, 320, 16);
            mk(w!("STATIC"), w!("Status:"), f, 0, 12, 132, 92, 16);
            mk(w!("STATIC"), w!("-"), f, IDC_P_STATUS, 108, 132, 320, 16);
            mk(w!("STATIC"), w!("Connections:"), f, 0, 12, 152, 92, 16);
            mk(w!("STATIC"), w!("-"), f, IDC_P_SEG, 108, 152, 320, 16);
            mk(
                w!("BUTTON"),
                w!("Stop"),
                (f | WS_TABSTOP.0) as u32,
                IDC_P_STOP,
                12,
                192,
                95,
                26,
            );
            mk(
                w!("BUTTON"),
                w!("Remove"),
                (f | WS_TABSTOP.0) as u32,
                IDC_P_REMOVE,
                115,
                192,
                95,
                26,
            );
            mk(
                w!("BUTTON"),
                w!("Close"),
                (f | WS_TABSTOP.0) as u32,
                IDC_P_CLOSE,
                329,
                192,
                95,
                26,
            );
            // Title static shows the file name.
            let mut title = vec![0u16; 512];
            let n = GetWindowTextW(hwnd, &mut title);
            let name = String::from_utf16_lossy(&title[..n.max(0) as usize]);
            SetWindowTextW(GetDlgItem(hwnd, IDC_P_FILE as i32).unwrap_or_default(), PCWSTR(wide(&name).as_ptr()));
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u16;
            let raw = hwnd.0 as isize;
            let dl_id = PROGRESS_WINS
                .lock()
                .unwrap()
                .iter()
                .find(|(h, _)| *h == raw)
                .map(|(_, d)| d.clone());
            match id {
                IDC_P_STOP => {
                    if let Some(dl_id) = dl_id {
                        std::thread::spawn(move || {
                            let _ = api::cancel_download(&dl_id);
                        });
                    }
                }
                IDC_P_REMOVE => {
                    if let Some(dl_id) = dl_id {
                        std::thread::spawn(move || {
                            let _ = api::delete_download(&dl_id);
                        });
                    }
                    let _ = DestroyWindow(hwnd);
                }
                IDC_P_CLOSE => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let raw = hwnd.0 as isize;
            PROGRESS_WINS
                .lock()
                .unwrap()
                .retain(|(h, _)| *h != raw);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn close_progress_for(id: &str) {
    let target = PROGRESS_WINS
        .lock()
        .unwrap()
        .iter()
        .find(|(_, did)| did == id)
        .map(|(h, _)| *h);
    if let Some(h) = target {
        unsafe {
            let _ = DestroyWindow(HWND(h as *mut _));
        }
    }
}

// ---------------------------------------------------------------------------
// Statistics window
// ---------------------------------------------------------------------------

unsafe fn open_stats(owner: HWND) {
    if let Some(raw) = *STATS_HWND.lock().unwrap() {
        let h = HWND(raw as *mut _);
        if IsWindow(h).as_bool() {
            let _ = SetForegroundWindow(h);
            return;
        }
    }
    let instance = GetModuleHandleW(None).unwrap_or_default();
    let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        CLASS_STATS,
        w!("Statistics"),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        660,
        420,
        owner,
        HMENU::default(),
        instance,
        None,
    ) else {
        return;
    };
    *STATS_HWND.lock().unwrap() = Some(hwnd.0 as isize);
    refresh_stats();
}

unsafe extern "system" fn stats_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let font = GetStockObject(DEFAULT_GUI_FONT);
            let list = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("SysListView32"),
                PCWSTR::null(),
                WINDOW_STYLE(
                    (WS_CHILD.0 | WS_VISIBLE.0 | LVS_REPORT | LVS_SHOWSELALWAYS) as u32,
                ),
                0,
                0,
                0,
                0,
                hwnd,
                HMENU(IDC_STATLIST as usize as *mut _),
                instance,
                None,
            )
            .unwrap_or_default();
            SendMessageW(list, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            SendMessageW(
                list,
                LVM_SETEXTENDEDLISTVIEWSTYLE,
                WPARAM(0),
                LPARAM((LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize),
            );
            const COLS: [(&str, i32); 5] = [
                ("Day", 110),
                ("Downloads", 90),
                ("Total size", 110),
                ("Top speed", 110),
                ("Fastest file", 280),
            ];
            for (i, (title, wdt)) in COLS.iter().enumerate() {
                insert_column(list, i as i32, title, *wdt);
            }
            *STATS_LIST.lock().unwrap() = Some(list.0 as isize);
            LRESULT(0)
        }
        WM_SIZE => {
            if let Some(raw) = *STATS_LIST.lock().unwrap() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                MoveWindow(
                    HWND(raw as *mut _),
                    0,
                    0,
                    rc.right - rc.left,
                    rc.bottom - rc.top,
                    BOOL::from(true),
                );
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            *STATS_HWND.lock().unwrap() = None;
            *STATS_LIST.lock().unwrap() = None;
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn refresh_stats() {
    let Some(raw) = *STATS_HWND.lock().unwrap() else { return };
    let Some(lraw) = *STATS_LIST.lock().unwrap() else { return };
    let list = HWND(lraw as *mut _);
    if !IsWindow(HWND(raw as *mut _)).as_bool() {
        return;
    }
    let daily = SNAPSHOT
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
        .daily;
    SendMessageW(list, LVM_DELETEALLITEMS, WPARAM(0), LPARAM(0));
    for (i, d) in daily.iter().enumerate() {
        let row = i as i32;
        lv_insert_row(list, row, &d.day);
        lv_set_cell(list, row, 1, &d.downloads.to_string());
        lv_set_cell(list, row, 2, &fmt_bytes(d.total_bytes));
        lv_set_cell(list, row, 3, &fmt_bps(d.top_bps));
        lv_set_cell(list, row, 4, d.top_speed_name.as_deref().unwrap_or("-"));
    }
}

// ---------------------------------------------------------------------------
// Custom draw: color Status (col 2) and Transfer rate (col 4) text
// ---------------------------------------------------------------------------

unsafe fn list_custom_draw(lparam: LPARAM) -> LRESULT {
    let cd = &*(lparam.0 as *const NMLVCUSTOMDRAW);
    let stage = cd.nmcd.dwDrawStage;
    if stage == CDDS_PREPAINT {
        return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
    }
    if stage == CDDS_ITEMPREPAINT {
        return LRESULT(CDRF_NOTIFYSUBITEMDRAW as isize);
    }
    if stage.0 & CDDS_SUBITEM.0 != 0 && stage.0 & CDDS_ITEMPREPAINT.0 != 0 {
        let sub = (*(lparam.0 as *const NMLVCUSTOMDRAW)).iSubItem;
        if sub == 2 || sub == 4 {
            let row = cd.nmcd.dwItemSpec as usize;
            if let Some(&color) = ROW_COLORS.lock().unwrap().get(row) {
                let cdw = &mut *(lparam.0 as *mut NMLVCUSTOMDRAW);
                cdw.clrText = COLORREF(color);
                return LRESULT(CDRF_NEWFONT as isize);
            }
        }
    }
    LRESULT(CDRF_DODEFAULT as isize)
}

// ---------------------------------------------------------------------------
// Poller + refresh
// ---------------------------------------------------------------------------

/// Background thread: poll the service and wake the UI thread each cycle.
fn poller(hwnd: HWND) {
    while !QUIT.load(Ordering::SeqCst) {
        let snap = Snapshot {
            ok: api::ping(),
            downloads: api::list_downloads().unwrap_or_default(),
            daily: api::daily_stats().unwrap_or_default(),
        };
        {
            let mut s = SNAPSHOT.lock().unwrap();
            *s = Some(snap);
        }
        unsafe {
            let _ = PostMessageW(hwnd, WM_APP_REFRESH, WPARAM(0), LPARAM(0));
        }
        std::thread::sleep(Duration::from_millis(900));
    }
}

/// UI thread: repaint tree counts, list, status bar, progress + stats windows.
unsafe fn refresh() {
    let Some(c) = *CTRL.lock().unwrap() else { return };
    let Some(snap) = SNAPSHOT.lock().unwrap().clone() else { return };
    let cat = CAT_SEL.load(Ordering::SeqCst);
    let search = SEARCH.lock().unwrap().clone();

    // Category counts (Default counts everything; others are subsets).
    let mut counts = [0usize; 6];
    counts[0] = snap.downloads.len();
    for d in &snap.downloads {
        counts[cat_of(&d.filename)] += 1;
    }
    for (i, &raw) in TREE_ITEMS.lock().unwrap().iter().enumerate() {
        let label = format!("{} ({})", CAT_NAMES[i], counts[i]);
        let mut t = wide(&label);
        let item = TVITEMW {
            mask: TVIF_TEXT,
            hItem: HTREEITEM(raw),
            pszText: PWSTR(t.as_mut_ptr()),
            ..Default::default()
        };
        SendMessageW(
            c.tree,
            TVM_SETITEMW,
            WPARAM(0),
            LPARAM(&item as *const TVITEMW as isize),
        );
    }

    // Filter + sort into display rows.
    let mut rows: Vec<&Download> = snap
        .downloads
        .iter()
        .filter(|d| cat == 0 || cat_of(&d.filename) == cat)
        .filter(|d| search.is_empty() || d.filename.to_lowercase().contains(&search))
        .collect();
    sort_rows(&mut rows);

    let ids: Vec<String> = rows.iter().map(|d| d.id.clone()).collect();
    let colors: Vec<u32> = rows.iter().map(|d| status_color(&d.status)).collect();

    let prev_sel = selected_download_id();
    let same_order = *ROW_IDS.lock().unwrap() == ids;
    if same_order {
        for (i, d) in rows.iter().enumerate() {
            fill_row(c.list, i as i32, d);
        }
    } else {
        SendMessageW(c.list, LVM_DELETEALLITEMS, WPARAM(0), LPARAM(0));
        for (i, d) in rows.iter().enumerate() {
            let row = i as i32;
            lv_insert_row(c.list, row, &d.filename);
            fill_row(c.list, row, d);
        }
        *ROW_IDS.lock().unwrap() = ids.clone();
        // Restore selection by id.
        if let Some(sel) = prev_sel {
            if let Some(idx) = ids.iter().position(|x| *x == sel) {
                let item = LVITEMW {
                    mask: LVIF_STATE,
                    iItem: idx as i32,
                    state: LVIS_SELECTED,
                    stateMask: LVIS_SELECTED,
                    ..Default::default()
                };
                SendMessageW(
                    c.list,
                    LVM_SETITEMSTATE,
                    WPARAM(idx as usize),
                    LPARAM(&item as *const LVITEMW as isize),
                );
                SendMessageW(
                    c.list,
                    LVM_ENSUREVISIBLE,
                    WPARAM(idx as usize),
                    LPARAM(0),
                );
            }
        }
    }
    *ROW_COLORS.lock().unwrap() = colors;

    // Status bar.
    let downloading = rows.iter().filter(|d| d.status == "running").count();
    sb_set(
        c.status,
        0,
        &format!(
            "{} file{} · {} downloading",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" },
            downloading
        ),
    );
    let today = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
    let t = snap.daily.iter().find(|d| d.day == today);
    match t {
        Some(d) => {
            sb_set(
                c.status,
                1,
                &format!(
                    "Today: {} files, {}",
                    d.downloads,
                    fmt_bytes(d.total_bytes)
                ),
            );
            sb_set(
                c.status,
                2,
                &format!("Top speed: {}", fmt_bps(d.top_bps)),
            );
        }
        None => {
            sb_set(c.status, 1, "Today: 0 files");
            sb_set(c.status, 2, "Top speed: —");
        }
    }
    sb_set(
        c.status,
        3,
        if snap.ok {
            "Service: online"
        } else {
            "Service: offline"
        },
    );

    // Detached progress windows.
    let wins: Vec<(isize, String)> = PROGRESS_WINS.lock().unwrap().clone();
    for (raw, id) in wins {
        let h = HWND(raw as *mut _);
        if IsWindow(h).as_bool() {
            update_progress_window(h, &id);
        }
    }

    // Statistics window.
    refresh_stats();
}

unsafe fn fill_row(list: HWND, row: i32, d: &Download) {
    lv_set_cell(list, row, 1, &if d.total_bytes > 0 {
        fmt_bytes(d.total_bytes)
    } else {
        "-".into()
    });
    lv_set_cell(list, row, 2, &status_text(d));
    let dl = if d.total_bytes > 0 {
        format!("{} ({}%)", fmt_bytes(d.downloaded_bytes), {
            let p = (d.downloaded_bytes.min(d.total_bytes)) * 100 / d.total_bytes;
            p
        })
    } else {
        fmt_bytes(d.downloaded_bytes)
    };
    lv_set_cell(list, row, 3, &dl);
    let rate = if d.status == "running" {
        fmt_bps(d.avg_bps)
    } else {
        "-".into()
    };
    lv_set_cell(list, row, 4, &rate);
    let eta = if d.status == "running" {
        fmt_time(eta_secs(d))
    } else {
        "-".into()
    };
    lv_set_cell(list, row, 5, &eta);
    let resume = if d.total_bytes > 0 { "Yes" } else { "No" };
    lv_set_cell(list, row, 6, resume);
}

// ---------------------------------------------------------------------------
// Categorization / sorting
// ---------------------------------------------------------------------------

fn cat_of(name: &str) -> usize {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "mp4" | "mkv" | "avi" | "webm" | "mov" | "flv" | "wmv" | "mpg" | "mpeg" | "m4v" | "ts" => 1,
        "exe" | "msi" | "msp" | "apk" | "jar" | "iso" => 2,
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" => 3,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "csv" | "epub"
        | "md" => 4,
        "zip" | "rar" | "7z" | "gz" | "bz2" | "tgz" | "tar" | "xz" | "cab" => 5,
        _ => 0,
    }
}

fn sort_rows(rows: &mut [&Download]) {
    match SORT.load(Ordering::SeqCst) {
        1 => rows.sort_by(|a, b| a.filename.to_lowercase().cmp(&b.filename.to_lowercase())),
        2 => rows.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes)),
        3 => rows.sort_by(|a, b| a.status.cmp(&b.status)),
        4 => rows.sort_by(|a, b| eta_secs(a).cmp(&eta_secs(b))),
        5 => rows.sort_by(|a, b| b.avg_bps.cmp(&a.avg_bps)),
        _ => {} // by order of addition (service order)
    }
}

fn eta_secs(d: &Download) -> u64 {
    if d.status == "running" && d.avg_bps > 0 && d.total_bytes > d.downloaded_bytes {
        (d.total_bytes - d.downloaded_bytes) / d.avg_bps
    } else {
        u64::MAX
    }
}

fn status_text(d: &Download) -> String {
    match d.status.as_str() {
        "running" => "Downloading".into(),
        "completed" => "Completed".into(),
        "cancelled" => "Stopped".into(),
        "failed" => format!("Error: {}", d.error.as_deref().unwrap_or("download failed")),
        other => other.to_string(),
    }
}

fn status_color(s: &str) -> u32 {
    match s {
        "running" => 0x0034_7A1F,  // green  RGB(31,122,52)
        "completed" => 0x003C_3C3C, // dark   RGB(60,60,60)
        "cancelled" => 0x0078_7878, // gray   RGB(120,120,120)
        "failed" => 0x002B_39C0,   // red    RGB(192,57,43)
        _ => 0x0000_0000,
    }
}

// ---------------------------------------------------------------------------
// List-view / status-bar helpers
// ---------------------------------------------------------------------------

unsafe fn insert_column(list: HWND, idx: i32, text: &str, width: i32) {
    let mut wtext = wide(text);
    let col = LVCOLUMNW {
        mask: LVCF_TEXT | LVCF_WIDTH,
        cx: width,
        pszText: PWSTR(wtext.as_mut_ptr()),
        ..Default::default()
    };
    SendMessageW(
        list,
        LVM_INSERTCOLUMNW,
        WPARAM(idx as usize),
        LPARAM(&col as *const _ as isize),
    );
}

unsafe fn lv_insert_row(lv: HWND, row: i32, text: &str) {
    let mut wtext = wide(text);
    let item = LVITEMW {
        mask: LVIF_TEXT,
        iItem: row,
        iSubItem: 0,
        pszText: PWSTR(wtext.as_mut_ptr()),
        ..Default::default()
    };
    SendMessageW(lv, LVM_INSERTITEMW, WPARAM(0), LPARAM(&item as *const _ as isize));
}

unsafe fn lv_set_cell(lv: HWND, row: i32, col: i32, text: &str) {
    let mut wtext = wide(text);
    let item = LVITEMW {
        mask: LVIF_TEXT,
        iItem: row,
        iSubItem: col,
        pszText: PWSTR(wtext.as_mut_ptr()),
        ..Default::default()
    };
    SendMessageW(
        lv,
        LVM_SETITEMTEXTW,
        WPARAM(row as usize),
        LPARAM(&item as *const _ as isize),
    );
}

unsafe fn sb_set(status: HWND, part: u32, text: &str) {
    let mut wtext = wide(text);
    SendMessageW(
        status,
        SB_SETTEXTW,
        WPARAM(part as usize),
        LPARAM(wtext.as_ptr() as isize),
    );
}

unsafe fn get_text(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    let n = GetWindowTextW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..n.max(0) as usize]).trim().to_string()
}

unsafe fn get_text_of(hwnd: HWND) -> String {
    get_text(hwnd)
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

fn fmt_bytes(n: u64) -> String {
    if n == 0 {
        return "0 B".into();
    }
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 || v >= 100.0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

fn fmt_bps(b: u64) -> String {
    if b == 0 {
        return "0 B/s".into();
    }
    format!("{}/s", fmt_bytes(b))
}

fn fmt_time(s: u64) -> String {
    if s == u64::MAX {
        return "-".into();
    }
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

/// NUL-terminated UTF-16, for Win32 string params.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------
// GDI-drawn toolbar / tree icons (magenta 0x00FF00FF becomes transparent)
// ---------------------------------------------------------------------------

const MAGENTA: COLORREF = COLORREF(0x00FF00FF);

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

unsafe fn fill_rect(hdc: HDC, l: i32, t: i32, r: i32, b: i32, col: COLORREF) {
    let brush = CreateSolidBrush(col);
    let rc = RECT {
        left: l,
        top: t,
        right: r,
        bottom: b,
    };
    FillRect(hdc, &rc, brush);
    let _ = DeleteObject(brush);
}

unsafe fn fill_circle(hdc: HDC, cx: i32, cy: i32, rad: i32, col: COLORREF) {
    let brush = CreateSolidBrush(col);
    let pen = CreatePen(PS_SOLID, 1, col);
    let ob = SelectObject(hdc, HGDIOBJ(brush.0));
    let op = SelectObject(hdc, HGDIOBJ(pen.0));
    Ellipse(hdc, cx - rad, cy - rad, cx + rad, cy + rad);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);
}

unsafe fn ring(hdc: HDC, cx: i32, cy: i32, rad: i32, col: COLORREF, thick: i32) {
    let nullb = GetStockObject(NULL_BRUSH);
    let pen = CreatePen(PS_SOLID, thick, col);
    let ob = SelectObject(hdc, nullb);
    let op = SelectObject(hdc, HGDIOBJ(pen.0));
    Ellipse(hdc, cx - rad, cy - rad, cx + rad, cy + rad);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    let _ = DeleteObject(pen);
}

unsafe fn tri(hdc: HDC, pts: &[POINT], col: COLORREF) {
    let brush = CreateSolidBrush(col);
    let pen = CreatePen(PS_SOLID, 1, col);
    let ob = SelectObject(hdc, HGDIOBJ(brush.0));
    let op = SelectObject(hdc, HGDIOBJ(pen.0));
    Polygon(hdc, pts);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);
}

unsafe fn line(hdc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, col: COLORREF, thick: i32) {
    let pen = CreatePen(PS_SOLID, thick, col);
    let op = SelectObject(hdc, HGDIOBJ(pen.0));
    MoveToEx(hdc, x1, y1, None);
    LineTo(hdc, x2, y2);
    SelectObject(hdc, op);
    let _ = DeleteObject(pen);
}

/// Draw icon `kind` into hdc at 0,0 sized `s`×`s` (drawn on a magenta ground).
unsafe fn draw_icon(hdc: HDC, kind: i32, s: i32) {
    let _ = s;
    let blue = rgb(47, 116, 208);
    let green = rgb(39, 174, 96);
    let red = rgb(192, 57, 43);
    let gray = rgb(124, 135, 148);
    let dark = rgb(70, 82, 94);
    let white = rgb(255, 255, 255);
    let amber = rgb(230, 126, 34);
    let purple = rgb(142, 68, 173);
    let brown = rgb(160, 90, 50);
    match kind {
        // 0 Add: blue circle + white plus + green badge with down arrow.
        0 => {
            fill_circle(hdc, 9, 9, 8, blue);
            fill_rect(hdc, 8, 4, 10, 14, white);
            fill_rect(hdc, 4, 8, 14, 10, white);
            fill_circle(hdc, 15, 15, 5, green);
            fill_rect(hdc, 14, 11, 16, 16, white);
            tri(hdc, &[POINT { x: 11, y: 14 }, POINT { x: 19, y: 14 }, POINT { x: 15, y: 19 }], white);
            fill_circle(hdc, 15, 15, 4, green);
            fill_rect(hdc, 14, 12, 16, 16, white);
        }
        // 1 Resume: green circle + white play triangle.
        1 => {
            fill_circle(hdc, 10, 10, 9, green);
            tri(
                hdc,
                &[
                    POINT { x: 7, y: 5 },
                    POINT { x: 16, y: 10 },
                    POINT { x: 7, y: 15 },
                ],
                white,
            );
        }
        // 2 Stop: red circle + white square.
        2 => {
            fill_circle(hdc, 10, 10, 9, red);
            fill_rect(hdc, 6, 6, 14, 14, white);
        }
        // 3 Stop all: red circle + two white bars.
        3 => {
            fill_circle(hdc, 10, 10, 9, red);
            fill_rect(hdc, 4, 6, 9, 14, white);
            fill_rect(hdc, 11, 6, 16, 14, white);
        }
        // 4 Remove: trash can.
        4 => {
            fill_rect(hdc, 5, 6, 15, 17, gray);
            fill_rect(hdc, 3, 3, 17, 5, dark);
            fill_rect(hdc, 8, 1, 12, 3, dark);
            fill_rect(hdc, 8, 8, 9, 15, white);
            fill_rect(hdc, 11, 8, 12, 15, white);
        }
        // 5 Options: gear-ish ring with spokes.
        5 => {
            fill_circle(hdc, 10, 10, 8, gray);
            fill_circle(hdc, 10, 10, 3, MAGENTA);
            for (dx, dy) in [(0, -9), (0, 9), (-9, 0), (9, 0)] {
                fill_rect(
                    hdc,
                    10 + dx - 2,
                    10 + dy - 2,
                    10 + dx + 2,
                    10 + dy + 2,
                    gray,
                );
            }
        }
        // 6 Scheduler: clock.
        6 => {
            ring(hdc, 10, 10, 8, blue, 2);
            line(hdc, 10, 10, 10, 4, dark, 2);
            line(hdc, 10, 10, 15, 11, dark, 2);
        }
        // 7 Queue: stacked bars.
        7 => {
            fill_rect(hdc, 2, 3, 18, 7, green);
            fill_rect(hdc, 2, 9, 18, 13, blue);
            fill_rect(hdc, 2, 15, 18, 19, green);
        }
        // 8 Limiter: gauge with needle.
        8 => {
            ring(hdc, 10, 11, 8, dark, 2);
            line(hdc, 10, 11, 5, 6, red, 2);
            fill_circle(hdc, 10, 11, 2, dark);
        }
        // Tree icons (16px): distinct per category.
        10 => {
            // Default: blue page + down arrow.
            fill_rect(hdc, 3, 2, 13, 14, blue);
            fill_rect(hdc, 7, 4, 9, 9, white);
            tri(
                hdc,
                &[
                    POINT { x: 4, y: 9 },
                    POINT { x: 12, y: 9 },
                    POINT { x: 8, y: 13 },
                ],
                white,
            );
        }
        11 => {
            // Video: purple circle + play.
            fill_circle(hdc, 8, 8, 7, purple);
            tri(
                hdc,
                &[
                    POINT { x: 6, y: 4 },
                    POINT { x: 12, y: 8 },
                    POINT { x: 6, y: 12 },
                ],
                white,
            );
        }
        12 => {
            // Programs: green window.
            fill_rect(hdc, 2, 3, 14, 13, green);
            fill_rect(hdc, 3, 4, 13, 6, white);
            fill_rect(hdc, 4, 8, 7, 11, white);
            fill_rect(hdc, 9, 8, 12, 11, white);
        }
        13 => {
            // Music: amber note.
            fill_circle(hdc, 6, 12, 3, amber);
            line(hdc, 8, 12, 8, 3, amber, 2);
            line(hdc, 8, 3, 13, 5, amber, 2);
        }
        14 => {
            // Documents: white page + gray lines.
            fill_rect(hdc, 3, 2, 13, 14, rgb(235, 240, 245));
            ring(hdc, 8, 8, 6, gray, 1);
            for y in [5, 8, 11] {
                fill_rect(hdc, 5, y, 11, y + 1, gray);
            }
        }
        15 => {
            // Compressed: brown box + zipper.
            fill_rect(hdc, 2, 4, 14, 13, brown);
            fill_rect(hdc, 7, 4, 9, 13, white);
            fill_rect(hdc, 6, 6, 10, 7, white);
            fill_rect(hdc, 6, 9, 10, 10, white);
        }
        _ => {}
    }
}

/// Create a top-down 32bpp DIB section; returns (bitmap, pixel bits as 0xAARRGGBW).
unsafe fn make_dib(hdc: HDC, s: i32) -> (HBITMAP, *mut u32) {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: s,
            biHeight: -s, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbmp = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, HANDLE::default(), 0)
        .unwrap_or_default();
    (hbmp, bits as *mut u32)
}

/// Draw icon `kind` into a 32bpp DIB and add it with a real alpha channel:
/// untouched magenta pixels become fully transparent, drawn pixels opaque.
unsafe fn blit_icon(hdc_mem: HDC, hbmp: HBITMAP, bits: *mut u32, himl: HIMAGELIST, kind: i32, s: i32) {
    let old = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
    fill_rect(hdc_mem, 0, 0, s, s, MAGENTA);
    draw_icon(hdc_mem, kind, s);
    SelectObject(hdc_mem, old);
    for i in 0..(s * s) as usize {
        let px = *bits.add(i);
        // COLORREF magenta 0x00FF00FF == DIB byte order B,G,R,A with same low 24 bits.
        *bits.add(i) = if (px & 0x00FF_FFFF) == 0x00FF_00FF {
            0 // transparent
        } else {
            px | 0xFF00_0000 // opaque
        };
    }
    ImageList_Add(himl, hbmp, HBITMAP::default());
}

unsafe fn make_toolbar_imagelist() -> HIMAGELIST {
    let s = 20;
    let himl = ImageList_Create(s, s, ILC_COLOR32, 12, 1);
    let screen = GetDC(HWND::default());
    let mem = CreateCompatibleDC(screen);
    let (bmp, bits) = make_dib(mem, s);
    for kind in 0..9 {
        blit_icon(mem, bmp, bits, himl, kind, s);
    }
    icon_self_check(himl, s, "toolbar");
    DeleteObject(bmp);
    DeleteDC(mem);
    ReleaseDC(HWND::default(), screen);
    himl
}

unsafe fn make_tree_imagelist() -> HIMAGELIST {
    let s = 16;
    let himl = ImageList_Create(s, s, ILC_COLOR32, 8, 1);
    let screen = GetDC(HWND::default());
    let mem = CreateCompatibleDC(screen);
    let (bmp, bits) = make_dib(mem, s);
    for kind in 10..16 {
        blit_icon(mem, bmp, bits, himl, kind, s);
    }
    icon_self_check(himl, s, "tree");
    DeleteObject(bmp);
    DeleteDC(mem);
    ReleaseDC(HWND::default(), screen);
    himl
}

/// Self-check: composite icon 0 of `himl` over white and report the corner
/// pixel (must stay white = icon is transparent there) + center pixel.
unsafe fn icon_self_check(himl: HIMAGELIST, s: i32, label: &str) {
    let screen = GetDC(HWND::default());
    let mem = CreateCompatibleDC(screen);
    let (bmp, bits) = make_dib(mem, s);
    let old = SelectObject(mem, HGDIOBJ(bmp.0));
    fill_rect(mem, 0, 0, s, s, rgb(255, 255, 255));
    let hicon = ImageList_GetIcon(himl, 0, ILD_TRANSPARENT);
    let _ = DrawIconEx(mem, 0, 0, hicon, s, s, 0, HBRUSH::default(), DI_NORMAL);
    SelectObject(mem, old);
    let corner = *bits;
    let center = *bits.add((s / 2 + (s / 2) * s) as usize);
    let line = format!(
        "{label}: corner={corner:#010x} (want 0x00ffffff) center={center:#010x} (want non-white, opaque)\n"
    );
    let _ = std::fs::write(
        std::env::temp_dir().join("dm-icon-check.txt"),
        format!(
            "{}{line}",
            std::fs::read_to_string(std::env::temp_dir().join("dm-icon-check.txt")).unwrap_or_default()
        ),
    );
    let _ = DestroyIcon(hicon);
    DeleteObject(bmp);
    DeleteDC(mem);
    ReleaseDC(HWND::default(), screen);
}
