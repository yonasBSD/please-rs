---
title: please.ini
section: 5
header: User Manual
footer: please 0.5.6
author: Ed Neville (ed-please@s5h.net)
date: 06 June 2025
---

# NAME

please.ini - configuration file for access

# DESCRIPTION

The **please.ini** file contains one or more **[sections]** that hold ACL for users of the **please** and **pleaseedit** programs.

`please.ini` is an ini file, sections can be named with a short description of what the section provides. You may then find this helpful when listing rights with **please -l**.

Rules are read and applied in the order they are presented in the configuration file. For example, if the user matches a permit rule to run a command in an early section, but in a later section matches criteria for a deny and no further matches, then the user will not be permitted to run that command. The last match wins.

The properties permitted are described below and should appear at most once per section. If a property is used more than once in a section, the last one will be used.

# SECTION OPTIONS

**[section-name]**
: section name, shown in list mode

**include=[file]**
: read ini file, and continue to next section

**includedir=[directory]**
: read .ini files in directory, and continue to next section, if the directory does not exist config parse will fail

Sections with a name starting **default** will retain match actions including implicit **permit**, therefore setting **permit=false** in the default block and **permit=true** elsewhere is advised.

# MATCHES

**name=[regex]**
: mandatory, the user or **group** (see below) to match against

**target=[regex]**
: user to execute or list as, defaults to **root**

**target_group=[regex]**
: requires that the user runs with **\-\-group** to run or edit with the match

**rule=[regex]**
: the regular expression that the command or edit path matches against, defaults to ^$

**notbefore=[YYYYmmdd|YYYYmmddHHMMSS]**
: will add HHMMSS as 00:00:00 to the date if not given, defaults to never

**notafter=[YYYYmmdd|YYYYmmddHHMMSS]**
: will add 23:59:59 to the date if not given, defaults to never

**datematch=[Day dd Mon HH:MM:SS UTC YYYY]**
: regex to match a date string with

**type=[edit/run/list]**
: this section's mode behaviour, defaults to **run**, edit = **pleaseedit** entry, list = user access rights listing

**group=[true|false]**
: defaults to false, when true, the **name** (above) refers to a group rather than a user

**hostname=[regex]**
: permitted hostnames where this may apply. A hostname defined as **any** or **localhost** will always match. Defaults to localhost

**dir=[regex]**
: permitted directories to run within

**permit_env=[regex]**
: allow environments that match **regex** to optionally pass through

**search_path=[string]**
: configure a **:** separated directory list to locate the binary to execute,  does not configure a **PATH** environment and is searched as the user running **please**, not as the **target** user (no plans to change that at present)

**regex** is a regular expression, **%{USER}** will expand to the user who is currently running `please`, **%{HOSTNAME}** expands to the hostname. See below for examples. Other **%{}** expansions may be added at a later date.

Spaces within arguments will be substituted as **'\\\ '** (backslash space). Use **^/bin/echo hello\\\\ world$** to match **/bin/echo "hello world"**, note that **\\** is a regex escape character so it must be escaped, therefore matching a space becomes **'\\\\\ '** (backslash backslash space).

To match a **\\** (backslash), the hex code **\\x5c** can be used.

To match the string **%{USER}**, the sequence **\\x25\\{USER\\}** can be used.

Rules starting **exact** are string matches and not **regex** processed and take precedence over **regex** matches.

**exact_name=[string]**
: only permit a user/group name that matches exactly

**exact_hostname=[string]**
: only permit a hostname that matches exactly

**exact_target=[string]**
: only permit a target that matches exactly

**exact_target_group=[groupname]**
: requires that the user runs with **\-\-group** to run or edit as **groupname**

**exact_rule=[string]**
: only permit a command rule that matches exactly

**exact_dir=[string]**
: only permit a dir that matches exactly

# ACTIONS

**permit=[true|false]**
: permit or disallow the entry, defaults to true

**require_pass=[true|false]**
: if entry matches, require a password, defaults to true

**timeout=[number]**
: length of timeout in whole seconds to wait for password input

**last=[true|false]**
: if true, stop processing when entry is matched, defaults to false

**reason=[true|false|regex]**
: require a reason for execution/edit. If reason is **true** then any reason will satisfy. Any string other than **true** or **false** will be treated as a regex match. Defaults to false

**token_timeout=[number]**
: length of timeout for token authentication in whole seconds (default 600)

**syslog=[true|false]**
: log this activity to syslog, defaults to true

**env_assign.[key]=[value]**
: assign **value** to environment **key**

**editmode=[octal mode|keep]**
: (**type=edit**) set the file mode bits on replacement file to octal mode. When set to **keep** use the existing file mode. If the file is not present, or mode is not declared, then mode falls back to 0600. If there is a file present, then the mode is read and used just prior to file rename

**exitcmd=[program]**
: (**type=edit**) run program after editor exits as the target user, if exit is zero, continue with file replacement. **%{NEW}** and **%{OLD}** placeholders expand to new and old edit files

# EXAMPLES

To allow all commands, you can use a greedy match (**^.\*$**). You should reduce this to the set of acceptable commands though.

```
[user_jim_root]
name = jim
target = root
rule = ^.*$
```

If you wish to permit a user to view another's command set, then you may do this using **type=list** (**run** by default). To list another user, they must match the **target** regex.

```
[user_jim_list_root]
name = jim
type = list
target = root
```

**type** may also be **edit** if you wish to permit a file edit with **pleaseedit**.

```
[user_jim_edit_hosts]
name = jim
type = edit
target = root
rule = ^/etc/hosts$
editmode = 644
```

Naming sections should help later when listing permissions.

Below, user **mandy** may run **du** without needing a password, but must enter her password for a **bash** running as root:

```
[mandy_du]
name = mandy
rule = ^(/usr)?/bin/du .*$
require_pass = false
[mandy_some]
name = mandy
rule = ^(/usr)?/bin/bash$
require_pass = true
```

The rule **regex** can include repetitions. To permit running **wc** to count the lines in the log files (we don't know how many there are) in **/var/log**. This sort of regex will allow multiple instances of a **()** group with **+**, which is used to define the character class **[a-zA-Z0-9-]+**, the numeric class **\d+** and the group near the end of the line. In other words, multiple instances of files in **/var/log** that may end in common log rotate forms **-YYYYMMDD** or **.N**.

This will permit commands such as the following, note how for efficiency find will combine arguments with **\+** into fewer invocations. **xargs** could have been used in place of **find**.

```
$ find /var/log -type f -exec please /usr/bin/wc {} \+
```

Here is a sample for the above scenario:

```
[user_jim_root_wc]
name = jim
target = root
permit = true
rule = ^/usr/bin/wc (/var/log/[a-zA-Z0-9-]+(\.\d+)?(\s)?)+$
```

User jim may only start or stop a docker container:

```
[user_jim_root_docker]
name = jim
target = root
permit = true
rule = ^/usr/bin/docker (start|stop) \S+
```

User ben may only edit **/etc/fstab**, and afterwards check the fstab file:

```
[ben_fstab]
name = ben
target = root
permit = true
type = edit
editmode = 644
rule = ^/etc/fstab$
exitcmd = /bin/findmnt --verify --tab-file %{NEW}
```

User ben may list only users **eng**, **net** and **dba**:

```
[ben_ops]
name = ben
permit = true
type = list
target = ^(eng|net|dba)ops$
```

All users may list their own permissions. You may or may not wish to do this if you consider permitting a view of the rules to be a security risk.

```
[list_own]
name = ^%{USER}$
permit = true
type = list
target = ^%{USER}$
```

# DEFAULT SECTION

Sections that are named starting with **default** retain their actions, which can be useful for turning off **syslog** or setting a **token_timeout** globally, for example, but they will retain **permit** which implicitly is **true**, it is therefore sensible to negate this (setting **permit=false**) and set **permit=true** in subsequent sections as needed.

```
[default:nosyslog]
name = .*
rule = .*
require_pass = false
syslog = false
permit = false
token_timeout = 1800
[mailusers]
name = mailadm
group = true
rule = ^/usr/sbin/postcat$
require_pass = true
permit = true
```

# EXITCMD

When the user completes their edit, and the editor exits cleanly, if **exitcmd** is included then this program will run as the target user. If the program also exits cleanly then the temporary edit will be copied to the destination.

**%{OLD}** and **%{NEW}** will expand to the old (existing source) file and edit candidate, respectively. To verify a file edit, **ben**'s entry to check **/etc/hosts** after clean exit could look like this:

```
[ben_ops]
name = ben
permit = true
type = edit
editmode = 644
rule = ^/etc/hosts$
exitcmd = /usr/local/bin/check_hosts %{OLD} %{NEW}
```

**/usr/local/bin/check_hosts** takes two arguments, the original file as the first argument and the modify candidate as the second argument. If **check_hosts** terminates zero, then the edit is considered clean and the original file is replaced with the candidate. Otherwise the edit file is not copied and is left, **pleaseedit** will exit with the return value from **check_hosts**.

A common **exitcmd** is to check the validity of **please.ini**, shown below. This permits members of the **admin** group to edit **/etc/please.ini** if they provide a reason (**-r**). Upon clean exit from the editor the tmp file will be syntax checked.

```
[please_ini]
name = admins
group = true
reason = true
rule = /etc/please.ini
type = edit
editmode = 600
exitcmd = /usr/bin/please -c %{NEW}
```

# DATED RANGES

For large environments it is not unusual for a third party to require access during a short time frame for debugging. To accommodate this there are the **notbefore** and **notafter** time brackets. These can be either **YYYYmmdd** or **YYYYmmddHHMMSS**.

The whole day is considered when using the shorter date form of **YYYYmmdd**.

Many enterprises may wish to permit periods of access to a user for a limited time only, even if that individual is considered to have a permanent role.

User joker can do what they want as root on 1st April 2021:

```
[joker_april_first]
name = joker
target = root
permit = true
notbefore = 20210401
notafter = 20210401
rule = ^/bin/bash
```

# DATEMATCHES

**datematch** matches against the date string **Day dd mon HH:MM:SS UTC Year**. This enables calendar style date matches.

Note that the day of the month (**dd**) will be padded with spaces if less than two characters wide.

You can permit a group of users to run **/usr/local/housekeeping/** scripts every Monday:

```
[l2_housekeeping]
name = l2users
group = true
target = root
permit = true
rule = /usr/local/housekeeping/tidy_(logs|images|mail)
datematch = ^Mon\s+.*
```

# REASONS

When **reason=true**, a user must pass a reason with the **-r** option to **please** and **pleaseedit**. Some organisations may prefer a reason to be logged when a command is executed. This can be helpful for some situations where something such as **mkfs** or **useradd** might be preferable to be logged against a ticket.

```
[l2_user_admin]
name = l2users
group = true
target = root
permit = true
reason = true
rule = ^/usr/sbin/useradd -m \w+$
```

Or, if tickets have a known prefix:

```
reason = .*(bug|incident|ticket|change)\d+.*
```

Perhaps you want to add a mini molly-guard where the hostname must appear in the reason:

```
[user_poweroff]
name = l2users
group = true
rule = (/usr)?/s?bin/(shutdown( -h now)?|poweroff|reboot)
require_pass = true
reason = .*%{HOSTNAME}.*
```

# DIR

In some situations you may only want a command to run within a set of directories. The directory is specified with the **-d** argument to **please**. For example, a program may output to the current working directory, which may only be desirable in certain locations.

```
[eng_build_aliases]
name = l2users
group = true
dir = ^/etc/mail$
rule = ^/usr/local/bin/build_aliases$
```

# LAST

**last = true** stops processing at a match:

```
[mkfs]
name = l2users
group = true
target = root
permit = true
reason = true
rule = ^/sbin/mkfs.(ext[234]|xfs) /dev/sd[bcdefg]\d?$
last = true
```

For simplicity, there is no need to process other configured rules if certain that the **l2users** group are safe to execute this. **last** should only be used in situations where there will never be something that could contradict the match in an undesired way later.

# SYSLOG

By default entries are logged to syslog. If you do not wish an entry to be logged then specify **syslog=false**. In this case **jim** can run anything in **/usr/bin/** as root and it will not be logged.

```
[maverick]
syslog = false
name = jim
rule = /usr/bin/.*
reason = false
```

# FILES

/etc/please.ini

# NOTES

At a later date repeated properties within the same section may be treated as a match list.

# CONTRIBUTIONS

I welcome pull requests with open arms. New features always considered.

# BUGS

Found a bug? Please either open a ticket or send a pull request/patch.

# SEE ALSO

**please**(1)
