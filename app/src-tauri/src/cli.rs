//! The command line of 15.0.0 (`src/main.cpp`, `src/cli/`): which role a
//! process plays, and what a CLI invocation asks the daemon to do.
//!
//! Parsing is pure: the working directory is a parameter, so the daemon can
//! re-parse forwarded arguments against the directory of the process that
//! sent them.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use phramer_core::config::{Color, Settings};
use regex::Regex;

/// How Windows starts Phramer for an `ms-screenclip:` link: this, then the
/// link as the one and only other argument.
const SCREENCLIP_ARGUMENT: &str = "--screenclip";

/// What this process is.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// No arguments: tray, hotkeys, stays running.
    Daemon,
    /// An `ms-screenclip:` recording link (Win+Shift+R), handed back to
    /// Snipping Tool.
    Recording,
    /// Anything else: parsed, then handled here or by the daemon. The first
    /// element is the program name.
    Command(Vec<String>),
}

/// Decides the role from the full argument list (program name first).
///
/// An `ms-screenclip` activation is read before the command-line parser and
/// everything after the link is dropped, so a crafted link cannot smuggle in
/// options: a link with quotes in it can split into more arguments, and then
/// it is not a link at all but still a plain capture.
pub(crate) fn classify(args: &[String]) -> Role {
    let Some(first) = args.get(1) else {
        return Role::Daemon;
    };
    if first != SCREENCLIP_ARGUMENT {
        return Role::Command(args.to_vec());
    }
    let link = if args.len() == 3 { args[2].as_str() } else { "" };
    if is_recording_request(link) {
        return Role::Recording;
    }
    let program = args.first().cloned().unwrap_or_default();
    Role::Command(vec![program, "gui".to_owned()])
}

/// Whether a link Windows handed over asks for a screen recording rather
/// than a snip (Print Screen, Win+Shift+S).
fn is_recording_request(link: &str) -> bool {
    let link = link.to_lowercase();
    link.contains("record") || link.contains("capture/video")
}

/// Outcome of a successful parse.
#[derive(Debug, PartialEq)]
pub(crate) enum Parsed {
    /// `-h`/`--help`: the text to print.
    Help(String),
    /// `-v`/`--version`.
    Version,
    Command(Command),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
    Gui(GuiRequest),
    Screen(ScreenRequest),
    Full(FullRequest),
    Launcher,
    Config(ConfigCommand),
}

impl Command {
    /// Whether the daemon carries this out. The rest only change the
    /// settings file, which 15.0.0 also did in the calling process.
    pub(crate) fn runs_in_daemon(&self) -> bool {
        !matches!(
            self,
            Command::Config(ConfigCommand::Check | ConfigCommand::Set(_))
        )
    }
}

/// A rectangle in physical pixels, `QRect::normalized()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

/// Where a capture is saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SaveTo {
    /// The configured folder or the save dialog.
    Default,
    /// An existing directory or a new file, absolute.
    Path(PathBuf),
}

/// What happens to the capture (`CaptureRequest::ExportTask`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Tasks {
    pub(crate) copy: bool,
    pub(crate) print_raw: bool,
    pub(crate) print_geometry: bool,
    pub(crate) pin: bool,
    pub(crate) accept_on_select: bool,
    pub(crate) save: Option<SaveTo>,
}

impl Tasks {
    fn is_empty(&self) -> bool {
        *self == Tasks::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum InitialSelection {
    #[default]
    None,
    Region(Rect),
    /// `--last-region`: whatever the previous capture selected.
    LastRegion,
}

/// An interactive capture (`gui`, the tray, the hotkeys).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GuiRequest {
    pub(crate) delay_ms: u32,
    pub(crate) initial_selection: InitialSelection,
    /// The tray's "Select Screen" entries; never set from the command line.
    pub(crate) monitor: Option<u32>,
    pub(crate) tasks: Tasks,
}

/// `screen`: one monitor, or with `--edit` an interactive capture on it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ScreenRequest {
    pub(crate) delay_ms: u32,
    /// `None`: the monitor under the cursor.
    pub(crate) monitor: Option<u32>,
    pub(crate) edit: bool,
    pub(crate) region: Option<Rect>,
    pub(crate) tasks: Tasks,
}

/// `full`: every monitor at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FullRequest {
    pub(crate) delay_ms: u32,
    pub(crate) tasks: Tasks,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConfigCommand {
    /// No options: open the settings window.
    Open,
    /// `--check`.
    Check,
    Set(ConfigChanges),
}

/// The settings `config` can change, in the order 15.0.0 applies them.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ConfigChanges {
    pub(crate) autostart: Option<bool>,
    pub(crate) notifications: Option<bool>,
    pub(crate) filename: Option<String>,
    /// `--trayicon`: whether the icon is shown.
    pub(crate) tray_icon: Option<bool>,
    pub(crate) main_color: Option<Color>,
    pub(crate) contrast_color: Option<Color>,
}

/// A command line 15.0.0 rejects.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum CliError {
    #[error("'{0}' is not a valid argument.")]
    NotAnArgument(String),
    #[error("the option {0} has a wrong format.")]
    WrongFormat(String),
    #[error("the option '{option}' is not a valid option for the argument '{argument}'.")]
    UnknownOption { option: String, argument: String },
    #[error("the option '{0}' contains a '=' and it doesn't require a value.")]
    UnexpectedValue(String),
    #[error("Expected value after the option '{0}'.")]
    MissingValue(String),
    #[error("{0}")]
    InvalidValue(String),
    #[error("Invalid arguments after the version option.")]
    AfterVersion,
    #[error("Invalid arguments after the help option.")]
    AfterHelp,
    #[error("The 'screen' command does not support '--region screen<N>'.")]
    ScreenRegion,
}

impl CliError {
    /// 15.0.0 exits 0 after a parse error (`goto finish`) and 1 only for a
    /// `screen` region it refuses after parsing.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            CliError::ScreenRegion => 1,
            _ => 0,
        }
    }
}

/// Printed after every parse error.
pub(crate) const GENERAL_ERROR: &str = "See phramer --help.";

const APP_NAME: &str = "phramer";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Opt {
    Path,
    Clipboard,
    Pin,
    Delay,
    LastRegion,
    Region,
    Filename,
    AcceptOnSelect,
    TrayIcon,
    Autostart,
    Notifications,
    Check,
    ShowHelp,
    MainColor,
    ContrastColor,
    Raw,
    PrintGeometry,
    Number,
    Edit,
    Help,
    Version,
}

#[derive(Clone, Copy)]
enum Checker {
    Any,
    Path,
    Delay,
    Region,
    Bool,
    Color,
    ScreenNumber,
}

struct OptionSpec {
    opt: Opt,
    names: &'static [&'static str],
    value_name: Option<&'static str>,
    description: &'static str,
    checker: Checker,
}

const fn spec(
    opt: Opt,
    names: &'static [&'static str],
    value_name: Option<&'static str>,
    description: &'static str,
    checker: Checker,
) -> OptionSpec {
    OptionSpec {
        opt,
        names,
        value_name,
        description,
        checker,
    }
}

const PATH: OptionSpec = spec(
    Opt::Path,
    &["p", "path"],
    Some("path"),
    "Existing directory or new file to save to",
    Checker::Path,
);
const CLIPBOARD: OptionSpec = spec(
    Opt::Clipboard,
    &["c", "clipboard"],
    None,
    "Save the capture to the clipboard",
    Checker::Any,
);
const PIN: OptionSpec = spec(
    Opt::Pin,
    &["pin"],
    None,
    "Pin the capture to the screen",
    Checker::Any,
);
const DELAY: OptionSpec = spec(
    Opt::Delay,
    &["d", "delay"],
    Some("milliseconds"),
    "Delay time in milliseconds",
    Checker::Delay,
);
const LAST_REGION: OptionSpec = spec(
    Opt::LastRegion,
    &["last-region"],
    None,
    "Repeat screenshot with previously selected region",
    Checker::Any,
);
const REGION: OptionSpec = spec(
    Opt::Region,
    &["region"],
    Some("WxH+X+Y or string"),
    "Screenshot region to select",
    Checker::Region,
);
const FILENAME: OptionSpec = spec(
    Opt::Filename,
    &["f", "filename"],
    Some("pattern"),
    "Set the filename pattern",
    Checker::Any,
);
const ACCEPT_ON_SELECT: OptionSpec = spec(
    Opt::AcceptOnSelect,
    &["s", "accept-on-select"],
    None,
    "Accept capture as soon as a selection is made",
    Checker::Any,
);
const TRAY_ICON: OptionSpec = spec(
    Opt::TrayIcon,
    &["t", "trayicon"],
    Some("bool"),
    "Enable or disable the trayicon",
    Checker::Bool,
);
const AUTOSTART: OptionSpec = spec(
    Opt::Autostart,
    &["a", "autostart"],
    Some("bool"),
    "Enable or disable run at startup",
    Checker::Bool,
);
const NOTIFICATIONS: OptionSpec = spec(
    Opt::Notifications,
    &["n", "notifications"],
    Some("bool"),
    "Enable or disable the notifications",
    Checker::Bool,
);
const CHECK: OptionSpec = spec(
    Opt::Check,
    &["check"],
    None,
    "Check the configuration for errors",
    Checker::Any,
);
const SHOW_HELP: OptionSpec = spec(
    Opt::ShowHelp,
    &["s", "showhelp"],
    Some("bool"),
    "Show the help message in the capture mode",
    Checker::Bool,
);
const MAIN_COLOR: OptionSpec = spec(
    Opt::MainColor,
    &["m", "maincolor"],
    Some("color-code"),
    "Define the main UI color",
    Checker::Color,
);
const CONTRAST_COLOR: OptionSpec = spec(
    Opt::ContrastColor,
    &["k", "contrastcolor"],
    Some("color-code"),
    "Define the contrast UI color",
    Checker::Color,
);
const RAW: OptionSpec = spec(
    Opt::Raw,
    &["r", "raw"],
    None,
    "Print raw PNG capture",
    Checker::Any,
);
const PRINT_GEOMETRY: OptionSpec = spec(
    Opt::PrintGeometry,
    &["g", "print-geometry"],
    None,
    "Print geometry of the selection in the format WxH+X+Y. Does nothing if raw is specified",
    Checker::Any,
);
const NUMBER: OptionSpec = spec(
    Opt::Number,
    &["n", "number"],
    Some("Screen number"),
    "Define the screen to capture (starting from 0),\ndefault: screen containing the cursor",
    Checker::ScreenNumber,
);
const EDIT: OptionSpec = spec(
    Opt::Edit,
    &["e", "edit"],
    None,
    "Interactively select and edit the screenshot region",
    Checker::Any,
);
const HELP: OptionSpec = spec(
    Opt::Help,
    &["h", "help"],
    None,
    "Displays this help",
    Checker::Any,
);
const VERSION: OptionSpec = spec(
    Opt::Version,
    &["v", "version"],
    None,
    "Displays version information",
    Checker::Any,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    Root,
    Gui,
    Screen,
    Full,
    Launcher,
    Config,
}

/// Subcommands under the root, in 15.0.0's order.
const SUBCOMMANDS: [(Node, &str, &str); 5] = [
    (Node::Gui, "gui", "Start a manual capture in GUI mode."),
    (
        Node::Screen,
        "screen",
        "Capture a screenshot of the specified monitor.",
    ),
    (
        Node::Full,
        "full",
        "Capture screenshot of all monitors at the same time.",
    ),
    (Node::Launcher, "launcher", "Open the capture launcher."),
    (Node::Config, "config", "Configure phramer."),
];

impl Node {
    fn name(self) -> &'static str {
        SUBCOMMANDS
            .iter()
            .find(|(node, _, _)| *node == self)
            .map_or(APP_NAME, |(_, name, _)| name)
    }

    fn options(self) -> &'static [OptionSpec] {
        match self {
            Node::Root | Node::Launcher => &[],
            Node::Gui => &[
                PATH,
                CLIPBOARD,
                DELAY,
                REGION,
                LAST_REGION,
                RAW,
                PRINT_GEOMETRY,
                PIN,
                ACCEPT_ON_SELECT,
            ],
            Node::Screen => &[NUMBER, EDIT, CLIPBOARD, PATH, DELAY, REGION, RAW, PIN],
            Node::Full => &[PATH, CLIPBOARD, DELAY, RAW],
            Node::Config => &[
                AUTOSTART,
                NOTIFICATIONS,
                FILENAME,
                TRAY_ICON,
                SHOW_HELP,
                MAIN_COLOR,
                CONTRAST_COLOR,
                CHECK,
            ],
        }
    }

    fn child(self, name: &str) -> Option<Node> {
        if self != Node::Root {
            return None;
        }
        SUBCOMMANDS
            .iter()
            .find(|(_, candidate, _)| *candidate == name)
            .map(|(node, _, _)| *node)
    }
}

fn is_help(arg: &str) -> bool {
    arg == "-h" || arg == "--help"
}

/// Options found so far; the first occurrence of an option wins, as with
/// `CommandLineParser::value`.
#[derive(Default)]
struct Found(HashMap<Opt, String>);

impl Found {
    fn is_set(&self, opt: Opt) -> bool {
        self.0.contains_key(&opt)
    }

    fn value(&self, opt: Opt) -> Option<&str> {
        self.0.get(&opt).map(String::as_str)
    }
}

/// Parses the full argument list (program name first) the way 15.0.0's
/// `CommandLineParser` does. Relative paths resolve against `cwd`.
pub(crate) fn parse(args: &[String], cwd: &Path) -> Result<Parsed, CliError> {
    if let Some(first) = args.get(1)
        && (first == "-v" || first == "--version")
    {
        return if args.len() == 2 {
            Ok(Parsed::Version)
        } else {
            Err(CliError::AfterVersion)
        };
    }

    let mut node = Node::Root;
    let mut found = Found::default();
    let mut index = 1;
    if let Some(help) = help_at(args, index, node)? {
        return Ok(help);
    }
    while index < args.len() {
        let arg = &args[index];
        if arg.starts_with('-') {
            index = parse_option(args, index, node, cwd, &mut found)?;
        } else {
            node = node
                .child(arg)
                .ok_or_else(|| CliError::NotAnArgument(arg.clone()))?;
            if let Some(help) = help_at(args, index + 1, node)? {
                return Ok(help);
            }
        }
        index += 1;
    }
    build(node, &found, cwd).map(Parsed::Command)
}

/// A help option at `index` is accepted only as the last argument.
fn help_at(args: &[String], index: usize, node: Node) -> Result<Option<Parsed>, CliError> {
    match args.get(index) {
        Some(arg) if is_help(arg) => {
            if index + 1 == args.len() {
                Ok(Some(Parsed::Help(help_text(&args[..index], node))))
            } else {
                Err(CliError::AfterHelp)
            }
        }
        _ => Ok(None),
    }
}

/// Handles the option at `index`; returns the index of the last argument it
/// consumed.
fn parse_option(
    args: &[String],
    mut index: usize,
    node: Node,
    cwd: &Path,
    found: &mut Found,
) -> Result<usize, CliError> {
    let raw = args[index].as_str();
    let (dashed, inline_value) = match raw.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (raw, None),
    };
    let long = dashed.starts_with("--");
    let length = dashed.chars().count();
    if (long && length <= 3) || (!long && length != 2) {
        return Err(CliError::WrongFormat(dashed.to_owned()));
    }
    let name = if long { &dashed[2..] } else { &dashed[1..] };
    let spec = node
        .options()
        .iter()
        .find(|spec| spec.names.contains(&name))
        .ok_or_else(|| CliError::UnknownOption {
            option: name.to_owned(),
            argument: node.name().to_owned(),
        })?;

    let Some(_) = spec.value_name else {
        if inline_value.is_some() {
            return Err(CliError::UnexpectedValue(name.to_owned()));
        }
        found.0.entry(spec.opt).or_default();
        return Ok(index);
    };
    let value = match inline_value.filter(|value| !value.is_empty()) {
        Some(value) => value.to_owned(),
        None => {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| CliError::MissingValue(name.to_owned()))?
        }
    };
    if !check(spec.checker, &value, cwd) {
        return Err(CliError::InvalidValue(checker_message(spec.checker)));
    }
    found.0.entry(spec.opt).or_insert(value);
    Ok(index)
}

fn checker_message(checker: Checker) -> String {
    let message = match checker {
        Checker::Any => "",
        Checker::Path => {
            "Invalid path, must be an existing directory or a new file in an existing directory"
        }
        Checker::Delay => "Invalid delay, it must be a number greater than 0",
        Checker::Region => "Invalid region, use 'WxH+X+Y' or 'all' or 'screen0/screen1/...'.",
        Checker::Bool => "Invalid value, it must be defined as 'true' or 'false'",
        Checker::Color => {
            "Invalid color, this flag supports the following formats:\n\
             - #RGB (each of R, G, and B is a single hex digit)\n\
             - #RRGGBB\n- #RRRGGGBBB\n\
             - #RRRRGGGGBBBB\n\
             - Named colors like 'blue' or 'red'\n\
             You may need to escape the '#' sign as in '\\#FFF'"
        }
        Checker::ScreenNumber => "Invalid screen number, it must be non negative",
    };
    // The parser completes the sentence if the message did not
    if message.ends_with('.') {
        message.to_owned()
    } else {
        format!("{message}.")
    }
}

fn check(checker: Checker, value: &str, cwd: &Path) -> bool {
    match checker {
        Checker::Any => true,
        Checker::Path => {
            let path = cwd.join(value);
            path.is_dir() || path.parent().is_none_or(Path::is_dir)
        }
        Checker::Delay | Checker::ScreenNumber => to_int(value).is_some_and(|n| n >= 0),
        Checker::Region => parse_region(value).is_some(),
        Checker::Bool => value == "true" || value == "false",
        Checker::Color => parse_color(value).is_some(),
    }
}

/// `QString::toInt`: base 10, surrounding whitespace ignored.
fn to_int(text: &str) -> Option<i32> {
    text.trim().parse().ok()
}

/// `QDir(path).absolutePath()`: absolute against `cwd`, with `.`, `..` and
/// repeated separators removed.
fn absolute_path(cwd: &Path, path: &str) -> PathBuf {
    let mut clean = PathBuf::new();
    for component in cwd.join(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other),
        }
    }
    clean
}

static REGION_PATTERN: LazyLock<Option<Regex>> = LazyLock::new(|| {
    // `Region::process`: four integers, searched for anywhere in the text
    Regex::new(r"(-?[0-9]+)[x,.\s](-?[0-9]+)[+,.\s]*(-?[0-9]+)[+,.\s]*(-?[0-9]+)").ok()
});

/// The C++ `Region` value handler: `WxH+X+Y` (and looser spellings),
/// normalized so width and height are not negative.
fn parse_region(text: &str) -> Option<Rect> {
    let captures = REGION_PATTERN.as_ref()?.captures(text)?;
    let number = |group: usize| -> Option<i32> { captures.get(group)?.as_str().parse().ok() };
    let (mut width, mut height) = (number(1)?, number(2)?);
    let (mut x, mut y) = (number(3)?, number(4)?);
    if width < 0 {
        x = x.checked_add(width)?;
        width = width.checked_neg()?;
    }
    if height < 0 {
        y = y.checked_add(height)?;
        height = height.checked_neg()?;
    }
    Some(Rect {
        x,
        y,
        width,
        height,
    })
}

/// `QColor(text)` accepted, and opaque: the check of `--maincolor` and
/// `--contrastcolor`.
fn parse_color(text: &str) -> Option<Color> {
    let color = match text.strip_prefix('#') {
        Some(hex) => parse_hex_color(hex)?,
        None => named_color(text)?,
    };
    (color.a == 255).then_some(color)
}

/// The hex forms `QColor::fromString` takes: `RGB`, `RRGGBB`, `AARRGGBB`,
/// `RRRGGGBBB`, `RRRRGGGGBBBB`.
fn parse_hex_color(hex: &str) -> Option<Color> {
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |index: usize, width: usize| -> Option<u8> {
        let value = u32::from_str_radix(hex.get(index * width..(index + 1) * width)?, 16).ok()?;
        let byte = match width {
            1 => value * 0x11,
            2 => value,
            _ => value >> (4 * (width - 2)),
        };
        u8::try_from(byte).ok()
    };
    match hex.len() {
        8 => Some(Color {
            a: channel(0, 2)?,
            r: channel(1, 2)?,
            g: channel(2, 2)?,
            b: channel(3, 2)?,
        }),
        len @ (3 | 6 | 9 | 12) => {
            let width = len / 3;
            Some(Color::rgb(
                channel(0, width)?,
                channel(1, width)?,
                channel(2, width)?,
            ))
        }
        _ => None,
    }
}

/// An SVG colour name, looked up through the settings reader, which knows
/// Qt's table: the same name is read under two keys with different
/// defaults, so equal results mean the name was understood.
fn named_color(name: &str) -> Option<Color> {
    // Letters and blanks only, so the text cannot change the INI around it
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphabetic() || c == ' ') {
        return None;
    }
    let settings = Settings::parse(format!("[General]\nuiColor={name}\ndrawColor={name}\n").as_bytes());
    let (ui, draw) = (settings.ui_color(), settings.draw_color());
    (ui == draw).then_some(ui)
}

fn build(node: Node, found: &Found, cwd: &Path) -> Result<Command, CliError> {
    let path = || {
        found
            .value(Opt::Path)
            .filter(|path| !path.is_empty())
            .map(|path| absolute_path(cwd, path))
    };
    let delay = || {
        found
            .value(Opt::Delay)
            .and_then(to_int)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    let region = || found.value(Opt::Region).and_then(parse_region);

    let command = match node {
        // The parser accepts only a help or version request without a
        // subcommand; both returned earlier
        Node::Root => return Err(CliError::NotAnArgument(String::new())),
        Node::Launcher => Command::Launcher,
        Node::Gui => {
            let path = path();
            let mut tasks = Tasks {
                copy: found.is_set(Opt::Clipboard),
                print_raw: found.is_set(Opt::Raw),
                print_geometry: found.is_set(Opt::PrintGeometry),
                pin: found.is_set(Opt::Pin),
                accept_on_select: found.is_set(Opt::AcceptOnSelect),
                save: path.clone().map(SaveTo::Path),
            };
            if tasks.accept_on_select
                && !tasks.copy
                && !tasks.print_raw
                && path.is_none()
                && !tasks.print_geometry
                && !tasks.pin
            {
                tasks.save = Some(SaveTo::Default);
            }
            let initial_selection = match region() {
                Some(rect) => InitialSelection::Region(rect),
                None if found.is_set(Opt::LastRegion) => InitialSelection::LastRegion,
                None => InitialSelection::None,
            };
            Command::Gui(GuiRequest {
                delay_ms: delay(),
                initial_selection,
                monitor: None,
                tasks,
            })
        }
        Node::Screen => {
            if found
                .value(Opt::Region)
                .is_some_and(|region| region.starts_with("screen"))
            {
                return Err(CliError::ScreenRegion);
            }
            let edit = found.is_set(Opt::Edit);
            let monitor = found
                .value(Opt::Number)
                .filter(|number| !number.starts_with('-'))
                .and_then(to_int)
                .and_then(|n| u32::try_from(n).ok());
            let mut tasks = Tasks {
                copy: found.is_set(Opt::Clipboard),
                print_raw: found.is_set(Opt::Raw),
                pin: found.is_set(Opt::Pin),
                save: path().map(SaveTo::Path),
                ..Tasks::default()
            };
            if !edit && tasks.is_empty() {
                tasks.save = Some(SaveTo::Default);
            }
            Command::Screen(ScreenRequest {
                delay_ms: delay(),
                monitor,
                edit,
                region: region(),
                tasks,
            })
        }
        Node::Full => {
            let mut tasks = Tasks {
                copy: found.is_set(Opt::Clipboard),
                print_raw: found.is_set(Opt::Raw),
                save: path().map(SaveTo::Path),
                ..Tasks::default()
            };
            if tasks.is_empty() {
                tasks.save = Some(SaveTo::Default);
            }
            Command::Full(FullRequest {
                delay_ms: delay(),
                tasks,
            })
        }
        Node::Config => Command::Config(config_command(found)),
    };
    Ok(command)
}

fn config_command(found: &Found) -> ConfigCommand {
    if found.is_set(Opt::Check) {
        return ConfigCommand::Check;
    }
    let flag = |opt| found.value(opt).map(|value| value == "true");
    let changes = ConfigChanges {
        autostart: flag(Opt::Autostart),
        notifications: flag(Opt::Notifications),
        filename: found.value(Opt::Filename).map(str::to_owned),
        tray_icon: found.value(Opt::TrayIcon).map(|value| value != "false"),
        main_color: found.value(Opt::MainColor).and_then(parse_color),
        contrast_color: found.value(Opt::ContrastColor).and_then(parse_color),
    };
    // `--showhelp` is accepted but never applied, and alone it opens the
    // settings window, as in 15.0.0
    if changes == ConfigChanges::default() {
        ConfigCommand::Open
    } else {
        ConfigCommand::Set(changes)
    }
}

/// `CommandLineParser::printHelp`. `args` are the arguments before the help
/// option, program name first.
fn help_text(args: &[String], node: Node) -> String {
    let subcommands: &[(Node, &str, &str)] = if node == Node::Root {
        &SUBCOMMANDS
    } else {
        &[]
    };
    let mut text = format!(
        "Usage: {} [{}-options] {}\n\n",
        args.join(" "),
        node.name(),
        if subcommands.is_empty() {
            ""
        } else {
            "[subcommands]"
        }
    );
    text.push_str(
        "Per default runs Phramer in the background and adds a tray icon for configuration.\n\n",
    );

    let mut options: Vec<&OptionSpec> = node.options().iter().collect();
    options.push(&HELP);
    if node == Node::Root {
        options.push(&VERSION);
    }
    let dashed: Vec<String> = options
        .iter()
        .map(|spec| {
            let names: Vec<String> = spec
                .names
                .iter()
                .map(|name| {
                    if name.chars().count() == 1 {
                        format!("-{name}")
                    } else {
                        format!("--{name}")
                    }
                })
                .collect();
            let mut joined = names.join(", ");
            if let Some(value) = spec.value_name {
                let _ = write!(joined, " <{value}>");
            }
            joined
        })
        .collect();
    let width = dashed
        .iter()
        .map(|text| text.chars().count())
        .chain(subcommands.iter().map(|(_, name, _)| name.len()))
        .max()
        .unwrap_or(0);
    let padding = format!("\n{}", " ".repeat(width + 4));

    text.push_str("Options:\n");
    for (spec, names) in options.iter().zip(&dashed) {
        let description = spec.description.replace('\n', &padding);
        let _ = writeln!(text, "  {names:<width$}  {description}");
    }
    if !subcommands.is_empty() {
        text.push_str("\nSubcommands:\n");
        for (_, name, description) in subcommands {
            let _ = writeln!(text, "  {name:<width$}  {description}");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("phramer.exe")
            .chain(list.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// A working directory with a subdirectory, as the shell scripts set up.
    fn workdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("phramer-cli-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("subdir"));
        dir
    }

    fn command(list: &[&str]) -> Command {
        match parse(&args(list), &workdir()) {
            Ok(Parsed::Command(command)) => command,
            other => panic!("{list:?} parsed as {other:?}"),
        }
    }

    fn error(list: &[&str]) -> CliError {
        match parse(&args(list), &workdir()) {
            Err(error) => error,
            other => panic!("{list:?} parsed as {other:?}"),
        }
    }

    fn save_default() -> Tasks {
        Tasks {
            save: Some(SaveTo::Default),
            ..Tasks::default()
        }
    }

    // --- role and the ms-screenclip rule ---

    #[test]
    fn no_arguments_is_the_daemon() {
        assert_eq!(classify(&args(&[])), Role::Daemon);
    }

    #[test]
    fn arguments_are_a_command() {
        assert_eq!(
            classify(&args(&["gui", "-c"])),
            Role::Command(args(&["gui", "-c"]))
        );
        // An empty argument is still an argument
        assert_eq!(classify(&args(&[""])), Role::Command(args(&[""])));
    }

    #[test]
    fn a_screenclip_snip_link_is_a_bare_gui() {
        let snip = args(&["--screenclip", "ms-screenclip:?source=PrintScreen"]);
        assert_eq!(classify(&snip), Role::Command(args(&["gui"])));
        assert_eq!(
            classify(&args(&["--screenclip", "ms-screenclip:snip"])),
            Role::Command(args(&["gui"]))
        );
    }

    #[test]
    fn everything_after_the_screenclip_link_is_dropped() {
        // A link that split into more arguments is not read as a link at
        // all, and none of the pieces reach the parser
        let smuggled = args(&[
            "--screenclip",
            "ms-screenclip:x",
            "--path",
            "C:\\evil",
            "--pin",
        ]);
        assert_eq!(classify(&smuggled), Role::Command(args(&["gui"])));
        let recording_split = args(&["--screenclip", "ms-screenclip:", "record"]);
        assert_eq!(classify(&recording_split), Role::Command(args(&["gui"])));
        assert_eq!(
            classify(&args(&["--screenclip"])),
            Role::Command(args(&["gui"]))
        );
    }

    #[test]
    fn a_screenclip_recording_link_goes_to_snipping_tool() {
        for link in [
            "ms-screenclip:?type=recording",
            "ms-screenclip://capture/video",
            "MS-SCREENCLIP:RECORD",
        ] {
            assert_eq!(
                classify(&args(&["--screenclip", link])),
                Role::Recording,
                "{link}"
            );
        }
    }

    #[test]
    fn screenclip_is_only_special_as_the_first_argument() {
        assert_eq!(
            error(&["gui", "--screenclip", "x"]),
            CliError::UnknownOption {
                option: "screenclip".into(),
                argument: "gui".into()
            }
        );
    }

    // --- tests/action_options.sh ---

    #[test]
    fn full_and_screen_with_path() {
        let dir = workdir();
        assert_eq!(
            command(&["full", "--path", dir.to_str().unwrap_or_default()]),
            Command::Full(FullRequest {
                delay_ms: 0,
                tasks: Tasks {
                    save: Some(SaveTo::Path(dir.clone())),
                    ..Tasks::default()
                }
            })
        );
        let Command::Screen(screen) = command(&["screen", "--path", dir.to_str().unwrap_or_default()])
        else {
            panic!("not a screen request");
        };
        assert_eq!(screen.tasks.save, Some(SaveTo::Path(dir)));
        assert_eq!(screen.monitor, None);
        assert!(!screen.edit);
    }

    #[test]
    fn full_and_screen_with_clipboard_or_raw() {
        for subcommand in ["full", "screen"] {
            for (option, expected) in [
                (
                    "--clipboard",
                    Tasks {
                        copy: true,
                        ..Tasks::default()
                    },
                ),
                (
                    "--raw",
                    Tasks {
                        print_raw: true,
                        ..Tasks::default()
                    },
                ),
            ] {
                let tasks = match command(&[subcommand, option]) {
                    Command::Full(request) => request.tasks,
                    Command::Screen(request) => request.tasks,
                    other => panic!("{other:?}"),
                };
                assert_eq!(tasks, expected, "{subcommand} {option}");
            }
        }
    }

    #[test]
    fn full_and_screen_without_tasks_save() {
        assert_eq!(
            command(&["full"]),
            Command::Full(FullRequest {
                delay_ms: 0,
                tasks: save_default()
            })
        );
        let Command::Screen(screen) = command(&["screen"]) else {
            panic!("not a screen request");
        };
        assert_eq!(screen.tasks, save_default());
    }

    #[test]
    fn screen_pin() {
        let Command::Screen(screen) = command(&["screen", "--pin"]) else {
            panic!("not a screen request");
        };
        assert_eq!(
            screen.tasks,
            Tasks {
                pin: true,
                ..Tasks::default()
            }
        );
    }

    #[test]
    fn gui_with_each_action() {
        let dir = workdir();
        let cases: Vec<(Vec<&str>, Tasks)> = vec![
            (
                vec!["gui", "--path", dir.to_str().unwrap_or_default()],
                Tasks {
                    save: Some(SaveTo::Path(dir.clone())),
                    ..Tasks::default()
                },
            ),
            (
                vec!["gui", "--clipboard"],
                Tasks {
                    copy: true,
                    ..Tasks::default()
                },
            ),
            (
                vec!["gui", "--print-geometry"],
                Tasks {
                    print_geometry: true,
                    ..Tasks::default()
                },
            ),
            (
                vec!["gui", "--pin"],
                Tasks {
                    pin: true,
                    ..Tasks::default()
                },
            ),
            (
                vec!["gui", "--raw"],
                Tasks {
                    print_raw: true,
                    ..Tasks::default()
                },
            ),
            (
                vec!["gui", "--clipboard", "--accept-on-select"],
                Tasks {
                    copy: true,
                    accept_on_select: true,
                    ..Tasks::default()
                },
            ),
            // Accept on select with nothing else to do saves
            (
                vec!["gui", "--accept-on-select"],
                Tasks {
                    accept_on_select: true,
                    save: Some(SaveTo::Default),
                    ..Tasks::default()
                },
            ),
            (
                vec!["gui", "-p", dir.to_str().unwrap_or_default(), "-c", "-r", "--pin"],
                Tasks {
                    copy: true,
                    print_raw: true,
                    pin: true,
                    save: Some(SaveTo::Path(dir.clone())),
                    ..Tasks::default()
                },
            ),
        ];
        for (list, tasks) in cases {
            assert_eq!(
                command(&list),
                Command::Gui(GuiRequest {
                    tasks,
                    ..GuiRequest::default()
                }),
                "{list:?}"
            );
        }
    }

    #[test]
    fn a_bare_gui_has_no_tasks() {
        assert_eq!(command(&["gui"]), Command::Gui(GuiRequest::default()));
    }

    #[test]
    fn the_raw_suffix_the_script_appends_is_accepted_everywhere_it_is_used() {
        // The script's wrapper appends --raw to every command it runs
        for list in [
            vec!["full", "--path", "./", "--raw"],
            vec!["screen", "--clipboard", "--raw"],
            vec!["screen", "--pin", "--raw"],
            vec!["gui", "--clipboard", "--accept-on-select", "--raw"],
            vec!["gui", "--accept-on-select", "--raw"],
        ] {
            assert!(
                matches!(parse(&args(&list), &workdir()), Ok(Parsed::Command(_))),
                "{list:?}"
            );
        }
    }

    // --- tests/path_option.sh ---

    #[test]
    fn a_path_in_a_missing_directory_is_rejected() {
        assert_eq!(
            error(&["screen", "-p", "blah/blah"]),
            CliError::InvalidValue(
                "Invalid path, must be an existing directory or a new file in an existing directory."
                    .into()
            )
        );
    }

    fn screen_save_path(list: &[&str]) -> PathBuf {
        match command(list) {
            Command::Screen(ScreenRequest {
                tasks:
                    Tasks {
                        save: Some(SaveTo::Path(path)),
                        ..
                    },
                ..
            }) => path,
            other => panic!("{list:?}: {other:?}"),
        }
    }

    #[test]
    fn a_relative_file_resolves_against_the_working_directory() {
        assert_eq!(
            screen_save_path(&["screen", "-p", "relative.png"]),
            workdir().join("relative.png")
        );
    }

    #[test]
    fn an_absolute_file_is_kept() {
        let target = workdir().join("absolute.png");
        assert_eq!(
            screen_save_path(&["screen", "-p", target.to_str().unwrap_or_default()]),
            target
        );
    }

    #[test]
    fn redundancy_in_the_path_is_removed() {
        let messy = format!(
            "{}/subdir/..///redundancy_removed.png",
            workdir().display()
        );
        assert_eq!(
            screen_save_path(&["screen", "-p", &messy]),
            workdir().join("redundancy_removed.png")
        );
    }

    #[test]
    fn a_directory_is_kept_as_the_directory() {
        assert_eq!(screen_save_path(&["screen", "-p", "./"]), workdir());
    }

    #[test]
    fn files_without_or_with_other_suffixes_are_accepted() {
        let dir = workdir();
        for name in ["without_suffix", "jpg_suffix.jpg", "absolute.png", "absolute"] {
            let target = dir.join(name);
            assert_eq!(
                screen_save_path(&["screen", "-p", target.to_str().unwrap_or_default()]),
                target
            );
        }
    }

    // --- main.cpp's remaining options ---

    #[test]
    fn gui_delay_region_and_last_region() {
        assert_eq!(
            command(&["gui", "-d", "250", "--region", "100x50+10+20"]),
            Command::Gui(GuiRequest {
                delay_ms: 250,
                initial_selection: InitialSelection::Region(Rect {
                    x: 10,
                    y: 20,
                    width: 100,
                    height: 50
                }),
                ..GuiRequest::default()
            })
        );
        assert_eq!(
            command(&["gui", "--last-region"]),
            Command::Gui(GuiRequest {
                initial_selection: InitialSelection::LastRegion,
                ..GuiRequest::default()
            })
        );
        // An explicit region wins over --last-region
        let Command::Gui(both) = command(&["gui", "--last-region", "--region=1x1+0+0"]) else {
            panic!("not gui");
        };
        assert!(matches!(both.initial_selection, InitialSelection::Region(_)));
    }

    #[test]
    fn regions_are_read_like_the_cpp_value_handler() {
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        assert_eq!(parse_region("100x50+10+20"), Some(rect(10, 20, 100, 50)));
        assert_eq!(parse_region("100,50 10.20"), Some(rect(10, 20, 100, 50)));
        assert_eq!(parse_region("-100x50+10+20"), Some(rect(-90, 20, 100, 50)));
        // Searched for anywhere, and backtracking splits digits when the
        // separators run out, as PCRE does
        assert_eq!(parse_region("1920x1080"), Some(rect(8, 0, 1920, 10)));
        assert_eq!(parse_region("all"), None);
        assert_eq!(parse_region("screen0"), None);
        assert_eq!(parse_region("99999999999x1+1+1"), None);
    }

    #[test]
    fn invalid_delays_are_rejected() {
        let message = "Invalid delay, it must be a number greater than 0.";
        assert_eq!(
            error(&["gui", "-d", "-5"]),
            CliError::InvalidValue(message.into())
        );
        assert_eq!(
            error(&["full", "--delay=soon"]),
            CliError::InvalidValue(message.into())
        );
    }

    #[test]
    fn screen_number_and_edit() {
        assert_eq!(
            command(&["screen", "-n", "1", "-e", "-d", "100"]),
            Command::Screen(ScreenRequest {
                delay_ms: 100,
                monitor: Some(1),
                edit: true,
                region: None,
                tasks: Tasks::default(),
            })
        );
        assert_eq!(
            error(&["screen", "--number", "-1"]),
            CliError::InvalidValue("Invalid screen number, it must be non negative.".into())
        );
    }

    #[test]
    fn a_screen_region_naming_a_screen_exits_with_1() {
        let error = error(&["screen", "--region", "screen1x2+3+4"]);
        assert_eq!(error, CliError::ScreenRegion);
        assert_eq!(error.exit_code(), 1);
    }

    #[test]
    fn launcher_takes_no_options() {
        assert_eq!(command(&["launcher"]), Command::Launcher);
        assert_eq!(
            error(&["launcher", "-c"]),
            CliError::UnknownOption {
                option: "c".into(),
                argument: "launcher".into()
            }
        );
    }

    #[test]
    fn config_without_options_opens_the_settings() {
        assert_eq!(command(&["config"]), Command::Config(ConfigCommand::Open));
        // --showhelp is accepted but does not count as a change
        assert_eq!(
            command(&["config", "-s", "true"]),
            Command::Config(ConfigCommand::Open)
        );
    }

    #[test]
    fn config_check_wins() {
        assert_eq!(
            command(&["config", "-a", "true", "--check"]),
            Command::Config(ConfigCommand::Check)
        );
    }

    #[test]
    fn config_changes() {
        assert_eq!(
            command(&[
                "config",
                "-a",
                "false",
                "-n",
                "true",
                "-f",
                "%F_%H-%M",
                "-t",
                "false",
                "-m",
                "#ff0000",
                "-k",
                "navy"
            ]),
            Command::Config(ConfigCommand::Set(ConfigChanges {
                autostart: Some(false),
                notifications: Some(true),
                filename: Some("%F_%H-%M".into()),
                tray_icon: Some(false),
                main_color: Some(Color::rgb(255, 0, 0)),
                contrast_color: Some(Color::rgb(0, 0, 128)),
            }))
        );
    }

    #[test]
    fn colors_are_checked_like_qcolor() {
        assert_eq!(parse_color("#f00"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(parse_color("#0000ff"), Some(Color::rgb(0, 0, 255)));
        assert_eq!(parse_color("#fff000000"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(parse_color("#ffff00000000"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(parse_color("#ff00ff00"), Some(Color::rgb(0, 255, 0)));
        assert_eq!(parse_color("Blue"), Some(Color::rgb(0, 0, 255)));
        // Translucent, unknown or malformed
        assert_eq!(parse_color("#8000ff00"), None);
        assert_eq!(parse_color("transparent"), None);
        assert_eq!(parse_color("notacolor"), None);
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("red;x"), None);
        assert!(matches!(
            error(&["config", "--maincolor", "nope"]),
            CliError::InvalidValue(message) if message.starts_with("Invalid color")
        ));
    }

    #[test]
    fn boolean_values_must_be_spelled_out() {
        assert_eq!(
            error(&["config", "--trayicon", "yes"]),
            CliError::InvalidValue("Invalid value, it must be defined as 'true' or 'false'.".into())
        );
    }

    #[test]
    fn the_first_occurrence_of_an_option_wins() {
        let Command::Full(full) = command(&["full", "-d", "10", "-d", "20"]) else {
            panic!("not full");
        };
        assert_eq!(full.delay_ms, 10);
    }

    #[test]
    fn option_format_errors() {
        assert_eq!(error(&["gui", "-"]), CliError::WrongFormat("-".into()));
        assert_eq!(error(&["gui", "--c"]), CliError::WrongFormat("--c".into()));
        assert_eq!(error(&["gui", "-cr"]), CliError::WrongFormat("-cr".into()));
        assert_eq!(
            error(&["gui", "--clipboard=yes"]),
            CliError::UnexpectedValue("clipboard".into())
        );
        assert_eq!(error(&["gui", "-d"]), CliError::MissingValue("d".into()));
        // Options before the subcommand belong to the root, which has none
        assert_eq!(
            error(&["-c", "gui"]),
            CliError::UnknownOption {
                option: "c".into(),
                argument: "phramer".into()
            }
        );
        assert_eq!(
            error(&["gui", "screen"]),
            CliError::NotAnArgument("screen".into())
        );
        assert_eq!(error(&["capture"]), CliError::NotAnArgument("capture".into()));
    }

    #[test]
    fn an_empty_inline_value_takes_the_next_argument() {
        let Command::Full(full) = command(&["full", "--delay=", "30"]) else {
            panic!("not full");
        };
        assert_eq!(full.delay_ms, 30);
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse(&args(&["--version"]), &workdir()), Ok(Parsed::Version));
        assert_eq!(parse(&args(&["-v"]), &workdir()), Ok(Parsed::Version));
        assert_eq!(error(&["-v", "gui"]), CliError::AfterVersion);
        assert_eq!(error(&["-h", "gui"]), CliError::AfterHelp);
        assert_eq!(error(&["gui", "-h", "-c"]), CliError::AfterHelp);
        // Only the root knows --version
        assert_eq!(
            error(&["gui", "--version"]),
            CliError::UnknownOption {
                option: "version".into(),
                argument: "gui".into()
            }
        );

        let Ok(Parsed::Help(root)) = parse(&args(&["--help"]), &workdir()) else {
            panic!("no root help");
        };
        assert!(root.starts_with("Usage: phramer.exe [phramer-options] [subcommands]\n\n"));
        assert!(root.contains("  -v, --version  Displays version information\n"));
        assert!(root.contains("\nSubcommands:\n  gui            Start a manual capture in GUI mode.\n"));

        let Ok(Parsed::Help(gui)) = parse(&args(&["gui", "-h"]), &workdir()) else {
            panic!("no gui help");
        };
        assert!(gui.starts_with("Usage: phramer.exe gui [gui-options] \n\n"));
        assert!(gui.contains(&format!("  -p, --path <path>{}Existing directory", " ".repeat(13))));
        assert!(!gui.contains("Subcommands"));

        let Ok(Parsed::Help(screen)) = parse(&args(&["screen", "--help"]), &workdir()) else {
            panic!("no screen help");
        };
        // A multi-line description continues under its first line
        assert!(screen.contains(
            "Define the screen to capture (starting from 0),\n                                default: screen containing the cursor\n"
        ));
    }

    #[test]
    fn which_commands_run_in_the_daemon() {
        assert!(command(&["gui"]).runs_in_daemon());
        assert!(command(&["screen"]).runs_in_daemon());
        assert!(command(&["full"]).runs_in_daemon());
        assert!(command(&["launcher"]).runs_in_daemon());
        assert!(command(&["config"]).runs_in_daemon());
        assert!(!command(&["config", "--check"]).runs_in_daemon());
        assert!(!command(&["config", "-t", "true"]).runs_in_daemon());
    }
}
