#!/usr/bin/env python3
"""Minimal AOSP init .rc parser (T725 Step 2a).

Faithful re-implementation of the AOSP init parser's *parse-time* behaviour,
grounded in the actual AOSP sources read for this lane:

  * system/core/init/parser.cpp  -- ParseConfigFile / ParseData loop
    (mo4tech Android 11.0.0_R25 analysis, quoting the AOSP source verbatim):
    - ParseConfigFile logs "Parsing file ..." then calls ParseData, then
      returns true REGARDLESS of per-line errors.
    - ParseData tokenizes line by line. A line starting a section
      (service/on/import) ends the previous section and starts a new one;
      a ParseSection error logs "filename:line: <error>", clears the
      section parser and sets bad_section_found (errors suppressed until
      the next valid section). A line inside a section goes to
      ParseLineSection; an error there logs "filename:line: <error>" and
      parsing CONTINUES with the next line -- the section stays active and
      previously parsed lines of the section stay registered.
    - Consequence: a single bad line can NEVER silently void a whole file.
      Sections parsed before the bad line remain registered.
  * system/core/init/tokenizer.cpp -- token rules: whitespace ' ', '\\t',
    '\\r' skipped; '#' starts a comment to end of line; double quotes group
    text; backslash escapes (\\n \\r \\t \\\\ \\<CR><LF> and \\<LF> = line
    continuation, unknown escape = literal char).
  * system/core/init/README.md (Android Init Language, current AOSP) --
    service syntax, option table, trigger forms, command table:
    - service <name> <pathname> [ <argument> ]*
    - on <trigger> [&& <trigger>]* ; property trigger property:name=value
      (value may be *); event trigger is a bare stage name.
    - exec [ <seclabel> [ <user> [ <group>* ] ] ] -- <command> [ <argument>* ]
      with seclabel '-' meaning default.
    - "The contents of /system/etc/init/ are alphabetized and parsed
      sequentially" (Imports section).
  * system/core/init/builtins.cpp -- GetBuiltinFunctionMap arg-count ranges
    (exec {1, kMax}, start {1,1}, ...), cross-checked against the README.

Scope: PARSE-TIME validation only. Runtime checks (do_exec seclabel lookup,
service binary existence, property evaluation) are NOT parse-time and are
not modelled here -- the real parser does not perform them either.

Usage: parse_init_rc.py <file.rc> [<file.rc> ...]
Exit 0: every file parsed with 0 errors (ACCEPT).
Exit 1: at least one error (REJECT -- errors listed with filename:line).
Output is a per-file verdict plus the registered sections summary.
"""

import re
import sys

# ---------------------------------------------------------------------------
# Tokenizer (system/core/init/tokenizer.cpp)
# ---------------------------------------------------------------------------

T_TEXT = "TEXT"
T_NEWLINE = "NEWLINE"
T_EOF = "EOF"


class Tokenizer:
    def __init__(self, data: bytes):
        # The C++ parser works on the raw bytes; .rc files are ASCII here.
        self.s = data.decode("utf-8", errors="surrogateescape")
        self.i = 0
        self.line = 0  # state.line: incremented on each T_NEWLINE delivered
        self.nexttoken = None

    def next_token(self):
        if self.nexttoken is not None:
            t = self.nexttoken
            self.nexttoken = None
            if t[0] == T_NEWLINE:
                # Deferred newline: the line increments when delivered,
                # matching the C++ tokenizer (state.line++ on T_NEWLINE).
                self.line += 1
            return t
        s, n = self.s, len(self.s)
        i = self.i
        while True:
            if i >= n:
                self.i = i
                return (T_EOF, None)
            c = s[i]
            if c == "\n":
                self.i = i + 1
                self.line += 1
                return (T_NEWLINE, None)
            if c in " \t\r":
                i += 1
                continue
            if c == "#":
                while i < n and s[i] != "\n":
                    i += 1
                if i < n and s[i] == "\n":
                    self.i = i + 1
                    self.line += 1
                    return (T_NEWLINE, None)
                self.i = i
                return (T_EOF, None)
            break
        # text:
        out = []
        while True:
            if i >= n:
                self.i = i
                return (T_TEXT, "".join(out))
            c = s[i]
            if c in " \t\r":
                i += 1
                self.i = i
                return (T_TEXT, "".join(out))
            if c == "\n":
                self.nexttoken = (T_NEWLINE, None)
                i += 1
                self.i = i
                return (T_TEXT, "".join(out))
            if c == '"':
                i += 1
                while True:
                    if i >= n:
                        self.i = i
                        return (T_EOF, None)
                    if s[i] == '"':
                        i += 1
                        break
                    out.append(s[i])
                    i += 1
                continue
            if c == "\\":
                i += 1
                if i >= n:
                    self.i = i
                    return (T_TEXT, "".join(out))
                e = s[i]
                if e == "n":
                    out.append("\n")
                    i += 1
                elif e == "r":
                    out.append("\r")
                    i += 1
                elif e == "t":
                    out.append("\t")
                    i += 1
                elif e == "\\":
                    out.append("\\")
                    i += 1
                elif e == "\r":
                    # \<CR><LF> -> line continuation
                    if i + 1 < n and s[i + 1] == "\n":
                        i += 2
                        self.line += 1
                        while i < n and s[i] in " \t":
                            i += 1
                        continue
                    i += 1
                    continue
                elif e == "\n":
                    # \<LF> -> line continuation
                    i += 1
                    self.line += 1
                    while i < n and s[i] in " \t":
                        i += 1
                    continue
                else:
                    # unknown escape -- just copy
                    out.append(e)
                    i += 1
                continue
            out.append(c)
            i += 1


# ---------------------------------------------------------------------------
# Keyword tables (README.md + builtins.cpp function map)
# ---------------------------------------------------------------------------

# command -> (min_args, max_args); args counted AFTER the command keyword.
# kMax = unbounded.
# NOTE: installkey / enter_default_mount_ns / init_user0 / remount_userdata
# are Android 14 builtins (builtins.cpp do_installkey etc.) absent from the
# current README's Commands list; they were added after the stock flown
# hw/init.rc (Android 14, SDK 34) failed the first positive-control run on
# exactly those four lines (692/713/1017/1343). Their arg counts are the
# observed usages in that file. The property-trigger regex allows an empty
# value for the same reason (hw/init.rc:1255/1257 use
# property:sys.init.perf_lsm_hooks="").
KMAX = 10 ** 9
COMMANDS = {
    "bootchart": (0, 1),
    "chmod": (2, 2),
    "chown": (3, 3),
    "class_start": (1, 1),
    "class_stop": (1, 1),
    "class_reset": (1, 1),
    "class_restart": (1, 2),
    "copy": (2, 2),
    "copy_per_line": (2, 2),
    "domainname": (1, 1),
    "enable": (1, 1),
    "enter_default_mount_ns": (0, 0),
    "exec": (1, KMAX),
    "exec_background": (1, KMAX),
    "exec_start": (1, 1),
    "export": (2, 2),
    "hostname": (1, 1),
    "ifup": (1, 1),
    "init_user0": (0, 0),
    "insmod": (1, KMAX),
    "installkey": (1, 1),
    "interface_start": (1, 1),
    "interface_restart": (1, 1),
    "interface_stop": (1, 1),
    "load_exports": (1, 1),
    "load_system_props": (0, 0),
    "load_persist_props": (0, 0),
    "loglevel": (1, 1),
    "mark_post_data": (0, 0),
    "mkdir": (1, 6),
    "mount_all": (0, 2),
    "mount": (3, KMAX),
    "parse_apex_configs": (0, 0),
    "perform_apex_config": (0, 0),
    "restart": (1, 2),
    "restorecon": (1, KMAX),
    "restorecon_recursive": (1, KMAX),
    "rm": (1, 1),
    "rmdir": (1, 1),
    "readahead": (1, 2),
    "remount_userdata": (0, 0),
    "setprop": (2, 2),
    "setrlimit": (3, 3),
    "start": (1, 1),
    "stop": (1, 1),
    "swapon_all": (0, 1),
    "swapoff": (1, 1),
    "symlink": (2, 2),
    "sysclktz": (1, 1),
    "trigger": (1, 1),
    "umount": (1, 1),
    "umount_all": (0, 1),
    "update_linker_config": (0, 0),
    "verity_update_state": (0, 0),
    "wait": (1, 2),
    "wait_for_prop": (2, 2),
    "write": (2, 2),
}

# service option -> (min_args, max_args); args counted AFTER the option keyword.
SERVICE_OPTIONS = {
    "capabilities": (0, KMAX),
    "class": (1, KMAX),
    "console": (0, 1),
    "critical": (0, 2),
    "disabled": (0, 0),
    "enter_namespace": (2, 2),
    "file": (2, 2),
    "gentle_kill": (0, 0),
    "group": (1, KMAX),
    "interface": (2, 2),
    "ioprio": (2, 2),
    "keycodes": (1, KMAX),
    "memcg.limit_in_bytes": (1, 1),
    "memcg.limit_percent": (1, 1),
    "memcg.soft_limit_in_bytes": (1, 1),
    "memcg.swappiness": (1, 1),
    "namespace": (1, 1),
    "onrestart": (1, KMAX),
    "oneshot": (0, 0),
    "oom_score_adjust": (1, 1),
    "override": (0, 0),
    "priority": (1, 1),
    "reboot_on_failure": (1, 1),
    "restart_period": (1, 1),
    "rlimit": (3, 3),
    "seclabel": (1, 1),
    "setenv": (2, 2),
    "shared_kallsyms": (0, 0),
    "shutdown": (1, 1),
    "sigstop": (0, 0),
    "socket": (3, 6),
    "stdio_to_kmsg": (0, 0),
    "task_profiles": (1, KMAX),
    "timeout_period": (1, 1),
    "updatable": (0, 0),
    "user": (1, 1),
    "writepid": (1, KMAX),
}

SERVICE_NAME_RE = re.compile(r"^[A-Za-z0-9_.\-]+$")
EVENT_TRIGGER_RE = re.compile(r"^[A-Za-z0-9_.\-*]+$")
PROPERTY_TRIGGER_RE = re.compile(r"^property:([A-Za-z0-9_.\-]+)=(.*)$")


class ParseError(Exception):
    pass


# ---------------------------------------------------------------------------
# Section parsers
# ---------------------------------------------------------------------------

class ServiceSection:
    def __init__(self, args, filename, line):
        # args: ['service', name, pathname, ...]
        if len(args) < 3:
            raise ParseError(
                "service requires a name and a pathname (got %d args)" % (len(args) - 1)
            )
        if not SERVICE_NAME_RE.match(args[1]):
            raise ParseError("invalid service name '%s'" % args[1])
        if not args[2].startswith("/"):
            raise ParseError("service pathname '%s' must be absolute" % args[2])
        self.name = args[1]
        self.pathname = args[2]
        self.args = args[3:]
        self.options = []  # (line, option, args)
        self.filename = filename
        self.line = line

    def parse_line(self, args, line):
        kw = args[0]
        if kw not in SERVICE_OPTIONS:
            raise ParseError("unknown service option '%s'" % kw)
        lo, hi = SERVICE_OPTIONS[kw]
        n = len(args) - 1
        if n < lo or n > hi:
            raise ParseError(
                "option '%s' takes %d..%s args, got %d"
                % (kw, lo, "kMax" if hi == KMAX else hi, n)
            )
        if kw == "onrestart":
            # onrestart <command ...>: the embedded command must be known.
            cmd = args[1]
            if cmd not in COMMANDS:
                raise ParseError("onrestart: unknown command '%s'" % cmd)
        self.options.append((line, kw, args[1:]))

    def end_section(self, known_names):
        if self.name in known_names:
            raise ParseError("ignored duplicate definition of service '%s'" % self.name)
        known_names.add(self.name)


class ActionSection:
    def __init__(self, args, filename, line):
        # args: ['on', trigger, ['&&', trigger]...]
        if len(args) < 2:
            raise ParseError("on requires at least one trigger")
        rest = args[1:]
        # Split on '&&' tokens.
        triggers = []
        cur = []
        for tok in rest:
            if tok == "&&":
                if not cur:
                    raise ParseError("empty trigger before '&&'")
                triggers.append(" ".join(cur))
                cur = []
            else:
                cur.append(tok)
        if cur:
            triggers.append(" ".join(cur))
        if not triggers:
            raise ParseError("on requires at least one trigger")
        for t in triggers:
            if t.startswith("property:"):
                m = PROPERTY_TRIGGER_RE.match(t)
                if not m:
                    raise ParseError("invalid property trigger '%s'" % t)
            else:
                if not EVENT_TRIGGER_RE.match(t) or " " in t:
                    raise ParseError("invalid event trigger '%s'" % t)
        self.triggers = triggers
        self.commands = []  # (line, args)
        self.filename = filename
        self.line = line

    def parse_line(self, args, line):
        kw = args[0]
        if kw not in COMMANDS:
            raise ParseError("unknown command '%s'" % kw)
        lo, hi = COMMANDS[kw]
        n = len(args) - 1
        if n < lo or n > hi:
            raise ParseError(
                "command '%s' takes %d..%s args, got %d"
                % (kw, lo, "kMax" if hi == KMAX else hi, n)
            )
        if kw in ("exec", "exec_background"):
            # Grammar: [ <seclabel> [ <user> [ <group>* ] ] ] -- <command> ...
            # '--' separates optional context from the command. If present,
            # at least one token must follow it.
            body = args[1:]
            if "--" in body:
                idx = body.index("--")
                if idx == len(body) - 1:
                    raise ParseError("exec: no command after '--'")
        self.commands.append((line, args))

    def end_section(self, known_names):
        pass


# ---------------------------------------------------------------------------
# Parser::ParseData (parser.cpp)
# ---------------------------------------------------------------------------

def parse_file(path):
    with open(path, "rb") as f:
        data = f.read()
    # parser.cpp ParseData: data->push_back('\n') -- guarantees the last
    # line is terminated and therefore processed.
    orig_len = len(data)
    data = data + b"\n"
    tok = Tokenizer(data)
    errors = []  # (line, message) -- LOG(ERROR) lines the real parser emits
    services = []
    actions = []
    imports = []
    known_names = set()
    section = None  # current ServiceSection | ActionSection | 'import' | None
    section_start_line = -1
    bad_section_found = False

    def end_section():
        nonlocal section, section_start_line, bad_section_found
        bad_section_found = False
        if section is None or section == "import":
            section = None
            section_start_line = -1
            return
        try:
            section.end_section(known_names)
        except ParseError as e:
            errors.append((section_start_line, str(e)))
        if isinstance(section, ServiceSection):
            services.append(section)
        elif isinstance(section, ActionSection):
            actions.append(section)
        section = None
        section_start_line = -1

    args = []
    while True:
        ttype, text = tok.next_token()
        if ttype == T_EOF:
            end_section()
            break
        if ttype == T_NEWLINE:
            line = tok.line  # state.line was already incremented: 1-based
            if not args:
                continue
            if args[0] == "import":
                end_section()
                if len(args) != 2:
                    errors.append((line, "import requires exactly one path"))
                    bad_section_found = True
                    section = None
                else:
                    imports.append((line, args[1]))
                    section = "import"
                    section_start_line = line
            elif args[0] == "service":
                end_section()
                try:
                    section = ServiceSection(args, path, line)
                    section_start_line = line
                except ParseError as e:
                    errors.append((line, str(e)))
                    section = None
                    bad_section_found = True
            elif args[0] == "on":
                end_section()
                try:
                    section = ActionSection(args, path, line)
                    section_start_line = line
                except ParseError as e:
                    errors.append((line, str(e)))
                    section = None
                    bad_section_found = True
            elif section is not None and section != "import":
                try:
                    section.parse_line(args, line)
                except ParseError as e:
                    errors.append((line, str(e)))
                    # NOTE: the real parser CONTINUES here -- the section
                    # stays active and later lines still register.
            elif not bad_section_found:
                errors.append((line, "Invalid section keyword found"))
            args = []
            continue
        # T_TEXT
        args.append(text)

    return {
        "path": path,
        "bytes": orig_len,
        "errors": errors,
        "services": services,
        "actions": actions,
        "imports": imports,
    }


def main(argv):
    if len(argv) < 2:
        print("usage: parse_init_rc.py <file.rc> [...]")
        return 2
    overall_ok = True
    for path in argv[1:]:
        try:
            r = parse_file(path)
        except OSError as e:
            print("%s: UNREADABLE: %s" % (path, e))
            overall_ok = False
            continue
        print("=== %s (%d bytes) ===" % (r["path"], r["bytes"]))
        for s in r["services"]:
            opts = ", ".join(
                "%s%s" % (kw, (" " + " ".join(a)) if a else "")
                for (_, kw, a) in s.options
            )
            print("  service '%s' %s (line %d) options: %s"
                  % (s.name, s.pathname, s.line, opts if opts else "(none)"))
        for a in r["actions"]:
            cmds = "; ".join(" ".join(c) for (_, c) in a.commands)
            print("  on %s (line %d) commands: %s"
                  % (" && ".join(a.triggers), a.line, cmds if cmds else "(none)"))
        for (ln, imp) in r["imports"]:
            print("  import %s (line %d)" % (imp, ln))
        if r["errors"]:
            overall_ok = False
            print("  ERRORS (%d):" % len(r["errors"]))
            for (ln, msg) in r["errors"]:
                print("    %s:%d: %s" % (r["path"], ln, msg))
            print("  VERDICT: REJECT")
        else:
            print("  VERDICT: ACCEPT (0 errors; %d service(s), %d action(s), %d import(s))"
                  % (len(r["services"]), len(r["actions"]), len(r["imports"])))
    return 0 if overall_ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
