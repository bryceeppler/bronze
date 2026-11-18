param([switch]$Force)

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

Add-Type -ReferencedAssemblies System.Windows.Forms, System.Drawing -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;

public static class Desk
{
    [StructLayout(LayoutKind.Sequential)] struct LastInput { public uint Size; public uint Time; }
    [StructLayout(LayoutKind.Sequential)] struct KeyInput { public ushort Vk; public ushort Scan; public uint Flags; public uint Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Sequential)] struct MouseInput { public int X; public int Y; public uint Data; public uint Flags; public uint Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Explicit)] struct AnyInput { [FieldOffset(0)] public MouseInput Mouse; [FieldOffset(0)] public KeyInput Key; }
    [StructLayout(LayoutKind.Sequential)] struct Input { public uint Type; public AnyInput Data; }
    delegate bool EachWindow(IntPtr window, IntPtr data);

    [DllImport("user32.dll")] static extern bool GetLastInputInfo(ref LastInput info);
    [DllImport("kernel32.dll")] static extern uint GetTickCount();
    [DllImport("user32.dll")] static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool GetUserObjectInformation(IntPtr obj, int index, StringBuilder name, int length, out int needed);
    [DllImport("user32.dll", SetLastError = true)] static extern uint SendInput(uint count, Input[] inputs, int size);
    [DllImport("user32.dll")] static extern uint MapVirtualKey(uint code, uint kind);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr window);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint from, uint to, bool attach);
    [DllImport("user32.dll")] static extern bool EnumWindows(EachWindow each, IntPtr data);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll", SetLastError = true)] static extern bool OpenClipboard(IntPtr owner);
    [DllImport("user32.dll")] static extern bool CloseClipboard();
    [DllImport("user32.dll")] static extern bool EmptyClipboard();
    [DllImport("user32.dll")] static extern uint EnumClipboardFormats(uint format);
    [DllImport("user32.dll")] static extern IntPtr GetClipboardData(uint format);
    [DllImport("user32.dll")] static extern IntPtr SetClipboardData(uint format, IntPtr memory);
    [DllImport("user32.dll")] public static extern uint GetClipboardSequenceNumber();
    [DllImport("user32.dll")] static extern bool IsClipboardFormatAvailable(uint format);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern uint RegisterClipboardFormat(string name);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalAlloc(uint flags, UIntPtr bytes);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr memory);
    [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr memory);
    [DllImport("kernel32.dll")] static extern UIntPtr GlobalSize(IntPtr memory);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalFree(IntPtr memory);
    [DllImport("shell32.dll")] static extern int SHQueryUserNotificationState(out int state);

    const uint CfUnicodeText = 13;
    static readonly uint[][] Convertible = { new uint[] { 13, 1, 7 }, new uint[] { 2, 8, 17 } };
    static uint injected = GetTickCount();

    public static uint IdleMs()
    {
        var info = new LastInput { Size = 8 };
        GetLastInputInfo(ref info);
        return unchecked(GetTickCount() - info.Time);
    }

    public static uint Now() { return GetTickCount(); }
    public static uint Since(uint tick) { return unchecked(GetTickCount() - tick); }

    public static bool RealInputSinceInjecting()
    {
        var info = new LastInput { Size = 8 };
        GetLastInputInfo(ref info);
        return unchecked((int)(info.Time - injected)) > 300;
    }

    public static bool Locked()
    {
        IntPtr desktop = OpenInputDesktop(0, false, 0x0001);
        if (desktop == IntPtr.Zero) return true;
        var name = new StringBuilder(256);
        int needed;
        bool named = GetUserObjectInformation(desktop, 2, name, name.Capacity * 2, out needed);
        CloseDesktop(desktop);
        return !named || name.ToString() != "Default";
    }

    public static IntPtr Foreground() { return GetForegroundWindow(); }

    public static bool Focus(IntPtr window)
    {
        for (int attempt = 0; attempt < 20 && GetForegroundWindow() != window; attempt++)
        {
            uint process;
            uint them = GetWindowThreadProcessId(GetForegroundWindow(), out process);
            uint me = GetCurrentThreadId();
            AttachThreadInput(me, them, true);
            BringWindowToTop(window);
            SetForegroundWindow(window);
            AttachThreadInput(me, them, false);
            Thread.Sleep(50);
        }
        return GetForegroundWindow() == window;
    }

    public static IntPtr FindWindow(int processId, string title)
    {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, data) =>
        {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            var text = new StringBuilder(256);
            GetWindowText(window, text, text.Capacity);
            if (owner == processId && text.ToString() == title) { found = window; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static bool FullScreenInFront()
    {
        int state;
        return SHQueryUserNotificationState(out state) == 0 && (state == 2 || state == 3);
    }

    public static Point Cursor() { Point point; GetCursorPos(out point); return point; }
    public static void MoveCursor(Point point) { SetCursorPos(point.X, point.Y); }
    public static IntPtr WindowAt(Point point) { return GetAncestor(WindowFromPoint(point), 2); }

    static readonly Dictionary<string, ushort> KeyCodes = new Dictionary<string, ushort>
    {
        { "LShift", 0xA0 }, { "RShift", 0xA1 }, { "Ctrl", 0xA2 }, { "Alt", 0xA4 }, { "C", 0x43 }, { "X", 0x58 },
    };

    public static void Play(IntPtr expected, string timeline)
    {
        if (RealInputSinceInjecting()) throw new InvalidOperationException("someone used the keyboard or mouse during the run; stopped");
        if (GetForegroundWindow() != expected) throw new InvalidOperationException("the test window lost focus; pressed nothing");
        foreach (string step in timeline.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries))
        {
            int pause;
            if (int.TryParse(step, out pause)) Thread.Sleep(pause);
            else if (step == "click") { Mouse(0x0002); Thread.Sleep(25); Mouse(0x0004); }
            else Key(KeyCodes[step.Substring(1)], step[0] == '+');
        }
        injected = GetTickCount();
    }

    static void Key(ushort key, bool down)
    {
        var input = new Input { Type = 1 };
        input.Data.Key = new KeyInput { Vk = key, Scan = (ushort)MapVirtualKey(key, 0), Flags = down ? 0u : 0x0002u };
        Send(input);
    }

    static void Mouse(uint flags)
    {
        var input = new Input { Type = 0 };
        input.Data.Mouse = new MouseInput { Flags = flags };
        Send(input);
    }

    static void Send(Input input)
    {
        if (SendInput(1, new[] { input }, Marshal.SizeOf(typeof(Input))) != 1)
            throw new InvalidOperationException("SendInput failed: " + Marshal.GetLastWin32Error());
        injected = GetTickCount();
    }

    static bool Open(IntPtr owner)
    {
        for (int attempt = 0; attempt < 50; attempt++)
        {
            if (OpenClipboard(owner)) return true;
            Thread.Sleep(10);
        }
        return false;
    }

    static byte[] Read(uint format)
    {
        IntPtr memory = GetClipboardData(format);
        if (memory == IntPtr.Zero) return null;
        int size = (int)GlobalSize(memory).ToUInt64();
        IntPtr data = GlobalLock(memory);
        if (size == 0 || data == IntPtr.Zero) return null;
        var bytes = new byte[size];
        Marshal.Copy(data, bytes, 0, size);
        GlobalUnlock(memory);
        return bytes;
    }

    static void Put(uint format, byte[] bytes)
    {
        IntPtr memory = GlobalAlloc(0x0002, (UIntPtr)bytes.Length);
        IntPtr data = GlobalLock(memory);
        Marshal.Copy(bytes, 0, data, bytes.Length);
        GlobalUnlock(memory);
        if (SetClipboardData(format, memory) == IntPtr.Zero) GlobalFree(memory);
    }

    static bool PlainMemory(uint format)
    {
        return format != 2 && format != 3 && format != 9 && format != 14
            && !(format >= 0x80 && format <= 0x8E) && !(format >= 0x200 && format <= 0x3FF);
    }

    public static List<KeyValuePair<uint, byte[]>> SaveClipboard()
    {
        var saved = new List<KeyValuePair<uint, byte[]>>();
        if (!Open(IntPtr.Zero)) throw new InvalidOperationException("cannot open the clipboard to save it");
        var kept = new List<uint>();
        for (uint format = EnumClipboardFormats(0); format != 0; format = EnumClipboardFormats(format))
        {
            int family = Array.FindIndex(Convertible, members => Array.IndexOf(members, format) >= 0);
            if (!PlainMemory(format) || (family >= 0 && kept.Exists(earlier => Array.IndexOf(Convertible[family], earlier) >= 0))) continue;
            kept.Add(format);
            byte[] bytes = Read(format);
            if (bytes != null) saved.Add(new KeyValuePair<uint, byte[]>(format, bytes));
        }
        CloseClipboard();
        return saved;
    }

    public static void RestoreClipboard(List<KeyValuePair<uint, byte[]>> saved)
    {
        if (!Open(IntPtr.Zero)) throw new InvalidOperationException("cannot open the clipboard to restore it");
        EmptyClipboard();
        foreach (var entry in saved) Put(entry.Key, entry.Value);
        if (saved.Count > 0) Put(RegisterClipboardFormat("ExcludeClipboardContentFromMonitorProcessing"), new byte[] { 0 });
        CloseClipboard();
    }

    public static void WriteClipboard(IntPtr owner, string text, string html)
    {
        if (!Open(owner)) throw new InvalidOperationException("cannot open the clipboard to write it");
        EmptyClipboard();
        Put(CfUnicodeText, Encoding.Unicode.GetBytes(text + "\0"));
        if (!string.IsNullOrEmpty(html)) Put(RegisterClipboardFormat("HTML Format"), Encoding.UTF8.GetBytes(html + "\0"));
        Put(RegisterClipboardFormat("ExcludeClipboardContentFromMonitorProcessing"), new byte[] { 0 });
        CloseClipboard();
    }

    public static string ClipboardText()
    {
        if (!Open(IntPtr.Zero)) return null;
        byte[] bytes = Read(CfUnicodeText);
        CloseClipboard();
        if (bytes == null) return null;
        string text = Encoding.Unicode.GetString(bytes);
        int end = text.IndexOf('\0');
        return end < 0 ? text : text.Substring(0, end);
    }

    public static bool ClipboardHas(string name)
    {
        return IsClipboardFormatAvailable(RegisterClipboardFormat(name));
    }

    public static EditWindow ShowEdit(string text) { return Show(() => new EditWindow(text)); }
    public static CopyWindow ShowCopy(string text) { return Show(() => new CopyWindow(text)); }
    public static FullScreenWindow ShowFullScreen(string text) { return Show(() => new FullScreenWindow(text)); }

    static T Show<T>(Func<T> make) where T : Form
    {
        T form = null;
        var shown = new ManualResetEvent(false);
        var thread = new Thread(() =>
        {
            form = make();
            form.Shown += (sender, args) => shown.Set();
            Application.Run(form);
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.IsBackground = true;
        thread.Start();
        if (!shown.WaitOne(10000)) throw new InvalidOperationException("a test window never showed");
        return form;
    }

    public static void Close(Form form)
    {
        if (form != null && !form.IsDisposed) form.Invoke(new Action(form.Close));
    }
}

public class EditWindow : Form
{
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr CreateWindowEx(uint style, string kind, string text, uint flags, int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr data);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool SetWindowText(IntPtr window, string text);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr SetFocus(IntPtr window);
    IntPtr box;

    public EditWindow(string text)
    {
        Text = "Bronze test - selected text";
        StartPosition = FormStartPosition.Manual;
        Bounds = new Rectangle(160, 160, 540, 220);
        HandleCreated += (sender, args) => box = CreateWindowEx(0x200, "EDIT", text, 0x50010180, 16, 16, 480, 26, Handle, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
        Activated += (sender, args) => SetFocus(box);
        Shown += (sender, args) => SelectText(text);
    }

    public Point Blank { get { return PointToScreen(new Point(ClientSize.Width / 2, ClientSize.Height - 30)); } }

    public void SelectText(string text)
    {
        Invoke(new Action(() => { SetWindowText(box, text); SetFocus(box); SendMessage(box, 0x00B1, IntPtr.Zero, new IntPtr(-1)); }));
    }
}

public class CopyWindow : Form
{
    readonly System.Windows.Forms.Timer tail = new System.Windows.Forms.Timer { Interval = 150 };
    public volatile string Copied;
    public volatile int Copies;

    public CopyWindow(string copied)
    {
        Copied = copied;
        Text = "Bronze test - copies on Ctrl+Insert";
        StartPosition = FormStartPosition.Manual;
        Bounds = new Rectangle(200, 420, 540, 220);
        KeyPreview = true;
        KeyDown += (sender, args) =>
        {
            if (args.KeyData != (Keys.Control | Keys.Insert)) return;
            Copies++;
            Desk.WriteClipboard(Handle, Copied, null);
            tail.Start();
        };
        tail.Tick += (sender, args) =>
        {
            tail.Stop();
            Desk.WriteClipboard(Handle, Copied, "<b>" + Copied + "</b>");
        };
    }
}

public class FullScreenWindow : CopyWindow
{
    public FullScreenWindow(string copied) : base(copied)
    {
        Text = "Bronze test - full screen";
        FormBorderStyle = FormBorderStyle.None;
        Bounds = Screen.PrimaryScreen.Bounds;
    }
}
'@

$state = Join-Path $env:TEMP 'bronze-real-window.tick'
$editText = 'Selected in a standard edit control'
$copyText = 'Copied by an app that hides its text'
$chordText = 'Copied after the chord was released'
$fullText = 'Copied from a full-screen window by the chord'
$finalText = 'Captured after the timelines that must not capture'

function Pass([string]$what) { Write-Host "real window: $what" }
function Fail([string]$what) { throw $what }

function WaitFor([scriptblock]$condition, [int]$ms = 5000) {
    $deadline = (Get-Date).AddMilliseconds($ms)
    while ((Get-Date) -lt $deadline) {
        if (& $condition) { return $true }
        Start-Sleep -Milliseconds 50
    }
    return [bool](& $condition)
}

function Notes {
    try { $doc = Get-Content -Raw (Join-Path $data 'notes.json') | ConvertFrom-Json } catch { return @() }
    @($doc.loose | ForEach-Object { $_.text }) + @($doc.sections | ForEach-Object { $_.notes | ForEach-Object { $_.text } })
}

function Captures { @(Get-Content $log -ErrorAction SilentlyContinue | Where-Object { $_ -match '^bronze: captured' }) }

function Refuse([string]$why) {
    Write-Host "real window: refused, $why"
    exit 2
}

function Focus($window, [string]$what) {
    if (-not [Desk]::Focus($window.Handle)) { Fail "could not bring $what to the front" }
}

function Captured([string]$text, [string]$source, [string]$what) {
    if (-not (WaitFor { (Notes) -contains $text })) { Fail "$what`: notes.json never gained '$text'" }
    $line = @(Captures)[-1]
    if ($line -notlike "*via $source") { Fail "$what`: expected a capture via $source, the log says '$line'" }
}

function Nothing([string]$what) {
    $before = @(Captures).Count
    $notes = @(Notes).Count
    Start-Sleep -Milliseconds 1200
    if (@(Captures).Count -ne $before -or @(Notes).Count -ne $notes) { Fail "$what captured: $(@(Captures)[-1])" }
    Pass "$what captured nothing"
}

if ([Desk]::Locked()) { Refuse 'the session is locked' }
$idle = [Desk]::IdleMs()
$ours = (Test-Path $state) -and ([Desk]::Since([uint32](Get-Content $state)) -le $idle + 1000)
if (-not $Force -and $idle -lt 180000 -and -not $ours) {
    Refuse "the desktop was used $([int]($idle / 1000)) s ago; run it after 3 idle minutes, or pass -Force"
}
if (Get-Process bronze -ErrorAction SilentlyContinue) { Refuse 'another Bronze is running, and a test gesture would capture into it' }

New-Item -ItemType Directory -Force out | Out-Null
$log = (Join-Path (Get-Location) 'out\real-window.log')
$data = Join-Path $env:TEMP ('bronze-real-window-' + [guid]::NewGuid().ToString('N'))
$clipboard = [Desk]::SaveClipboard()
$cursor = [Desk]::Cursor()
$bronze = $null
$edit = $null
$copy = $null
$full = $null
$code = 0

try {
    & cargo build -q
    if ($LASTEXITCODE -ne 0) { Fail 'cargo build' }
    $bronze = Start-Process -FilePath 'target\debug\bronze.exe' -ArgumentList "--data `"$data`"" -RedirectStandardError $log `
        -RedirectStandardOutput 'out\real-window.out' -NoNewWindow -PassThru
    if (-not (WaitFor { [Desk]::FindWindow($bronze.Id, 'Bronze') -ne [IntPtr]::Zero -and (Get-Content $log -ErrorAction SilentlyContinue) -match 'listening for Shift gestures' } 15000)) {
        Fail 'Bronze never showed its window and started listening'
    }
    $panel = [Desk]::FindWindow($bronze.Id, 'Bronze')
    if (-not [Desk]::IsWindowVisible($panel)) { Fail 'the Bronze window is not visible' }
    if ((Get-Content $log) -match 'cannot register') { Fail "a chord is taken: $((Get-Content $log) -match 'cannot register')" }
    Pass 'Bronze shows its window and listens for Shift gestures'

    $edit = [Desk]::ShowEdit($editText)
    Focus $edit 'the edit window'
    [Desk]::WriteClipboard([IntPtr]::Zero, 'clipboard before the edit capture', $null)
    $sequence = [Desk]::GetClipboardSequenceNumber()
    [Desk]::Play($edit.Handle, '+LShift 40 -LShift 120 +LShift 40 -LShift')
    Captured $editText 'FocusedApp' 'double-tap in an edit control'
    if ([Desk]::ClipboardText() -ne 'clipboard before the edit capture' -or [Desk]::GetClipboardSequenceNumber() -ne $sequence) {
        Fail 'the edit capture touched the clipboard'
    }
    Pass 'double-tap Left Shift captured the edit control selection via FocusedApp and left the clipboard alone'

    $copy = [Desk]::ShowCopy($copyText)
    Focus $copy 'the copy window'
    [Desk]::WriteClipboard([IntPtr]::Zero, 'clipboard before the copy capture', $null)
    [Desk]::Play($copy.Handle, '+LShift 40 -LShift 120 +LShift 40 -LShift')
    Captured $copyText 'PressedCopy' 'double-tap in a window that hides its text'
    Start-Sleep -Milliseconds 600
    if ($copy.Copies -ne 1) { Fail "the copy window saw $($copy.Copies) Ctrl+Insert presses, not 1" }
    if ([Desk]::ClipboardText() -ne 'clipboard before the copy capture' -or [Desk]::ClipboardHas('HTML Format')) {
        Fail "the clipboard was not put back after the late second write: '$([Desk]::ClipboardText())'"
    }
    Pass 'double-tap Left Shift pressed Ctrl+Insert once, captured via PressedCopy and put the clipboard back after the late second write'

    $copy.Copied = $chordText
    Focus $copy 'the copy window'
    [Desk]::WriteClipboard([IntPtr]::Zero, 'clipboard before the chord capture', $null)
    [Desk]::Play($copy.Handle, '+Ctrl 60 +Alt 60 +C 50 -C 400 -Alt 80 -Ctrl')
    Captured $chordText 'PressedCopy' 'ctrl+alt+c held'
    Start-Sleep -Milliseconds 600
    if ($copy.Copies -ne 2) { Fail "the copy window saw $($copy.Copies - 1) clean Ctrl+Insert presses for the chord, not 1" }
    if ([Desk]::ClipboardText() -ne 'clipboard before the chord capture') { Fail 'the chord capture did not put the clipboard back' }
    Pass 'ctrl+alt+c held for 400 ms waited for the release, then captured via PressedCopy and put the clipboard back'

    Focus $edit 'the edit window'
    [Desk]::Play($edit.Handle, '+RShift 40 +LShift 60 -LShift 30 -RShift')
    if (-not (WaitFor { -not [Desk]::IsWindowVisible($panel) })) { Fail 'Right Shift + Left Shift did not hide Bronze' }
    Pass 'Right Shift + Left Shift hid Bronze'
    Focus $edit 'the edit window'
    [Desk]::Play($edit.Handle, '+RShift 40 +LShift 60 -LShift 30 -RShift')
    if (-not (WaitFor { [Desk]::IsWindowVisible($panel) })) { Fail 'Right Shift + Left Shift did not show Bronze again' }
    Pass 'Right Shift + Left Shift showed it again'

    $edit.SelectText($editText)
    Focus $edit 'the edit window'
    $blank = $edit.Blank
    [Desk]::MoveCursor($blank)
    if ([Desk]::WindowAt($blank) -ne $edit.Handle) { Fail 'the edit window is not under the cursor for the Shift-clicks' }
    [Desk]::Play($edit.Handle, '+LShift 30 click 30 -LShift 70 +LShift 30 click 30 -LShift')
    Nothing 'Shift-click twice'

    Focus $edit 'the edit window'
    [Desk]::Play($edit.Handle, '+LShift 40 -LShift 600 +LShift 40 -LShift')
    Nothing 'a slow double tap'

    Focus $edit 'the edit window'
    [Desk]::Play($edit.Handle, '+LShift 40 -LShift 60 +X 20 -X 60 +LShift 40 -LShift')
    Nothing 'Shift, a letter, then Shift'

    $full = [Desk]::ShowFullScreen($fullText)
    Focus $full 'the full-screen window'
    if (-not (WaitFor { [Desk]::FullScreenInFront() })) { Fail 'the shell never reported the test window as a full-screen app' }
    [Desk]::WriteClipboard([IntPtr]::Zero, 'clipboard before the full-screen chord', $null)
    [Desk]::Play($full.Handle, '+LShift 40 -LShift 120 +LShift 40 -LShift')
    Nothing 'a double tap over a full-screen app'
    if ($full.Copies -ne 0) { Fail 'the double tap pressed Ctrl+Insert in the full-screen app' }
    Focus $full 'the full-screen window'
    [Desk]::Play($full.Handle, '+RShift 40 +LShift 60 -LShift 30 -RShift')
    Start-Sleep -Milliseconds 1000
    if (-not [Desk]::IsWindowVisible($panel)) { Fail 'Right Shift + Left Shift hid Bronze over a full-screen app' }
    Pass 'Right Shift + Left Shift over a full-screen app left Bronze shown'
    Focus $full 'the full-screen window'
    [Desk]::Play($full.Handle, '+Ctrl 60 +Alt 60 +C 50 -C 80 -Alt 80 -Ctrl')
    Captured $fullText 'PressedCopy' 'ctrl+alt+c over a full-screen app'
    Start-Sleep -Milliseconds 600
    if ($full.Copies -ne 1) { Fail "the full-screen app saw $($full.Copies) Ctrl+Insert presses, not 1" }
    if ([Desk]::ClipboardText() -ne 'clipboard before the full-screen chord') { Fail 'the full-screen chord capture did not put the clipboard back' }
    Pass 'ctrl+alt+c over a full-screen app still captured via PressedCopy and put the clipboard back'
    [Desk]::Close($full)
    $full = $null
    if (-not (WaitFor { -not [Desk]::FullScreenInFront() })) { Fail 'the shell still reports a full-screen app after closing the test window' }

    $edit.SelectText($finalText)
    Focus $edit 'the edit window'
    [Desk]::Play($edit.Handle, '+LShift 40 -LShift 120 +LShift 40 -LShift')
    Captured $finalText 'FocusedApp' 'a clean double tap after the timelines and the full-screen app'
    Pass 'and once the full-screen app closed, a clean double tap captured again'
} catch {
    $failure = $_.Exception
    while ($failure.InnerException) { $failure = $failure.InnerException }
    Write-Host "real window: FAILED $($failure.Message)"
    if (Test-Path $log) { Get-Content $log | ForEach-Object { Write-Host "  $_" } }
    $code = 1
} finally {
    [Desk]::Close($full)
    [Desk]::Close($copy)
    [Desk]::Close($edit)
    if ($bronze) { Stop-Process -Id $bronze.Id -Force -ErrorAction SilentlyContinue; $bronze.WaitForExit(5000) | Out-Null }
    [Desk]::RestoreClipboard($clipboard)
    [Desk]::MoveCursor($cursor)
    Set-Content $state ([Desk]::Now())
    Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue
}
if ($code -eq 0) { Pass 'ok, the clipboard and the cursor are as they were' }
exit $code
