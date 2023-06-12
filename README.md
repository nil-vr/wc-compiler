This tool compiles events from multiple files into a format that can be loaded by VRChat.

[wc-undou] can be used an example of how to define events and set up GitHub actions to publish the calendar data.

[wc-undou]: https://github.com/nil-vc/wc-undou

# Events

Events are described using [toml] files. A daily event looks something like this:

```toml
# This is an IANA time zone name: https://en.wikipedia.org/wiki/Tz_database
# Events in time zones affected by daylight saving will change their times accordingly.
timezone = "America/New_York"
start = "17:00"
duration = "1:00"

# The rest are optional.

description = "This is my cool event."
# The ID of the VRChat group:
group = "MYGRP.2493"
# If you don't specify the supported platforms, PC is assumed.
platforms = ["pc", "quest"]
# The event's hashtag for social networks:
hashtag = "MyEvent"
# A link to the event's website:
web = "https://example.com/"
# The Twitter handle of the official Twitter account for the event.
twitter = "MyEvent"
# The join code of the Discord server.
discord = "nRszqyu"

# If the event is in a public instance, specify the world.
[world]
# This is the ID of the world for viewing on the website.
id = "wrld_a97970e3-8d89-41ae-82d8-6340e29385df"
# This is the name of the world if the user searches for it in VRChat.
name = "My event world"

# If the event is friends+ or friends-only, list the organizers.
[[join]]
# This is the ID of the organizer for viewing on the website.
id = "usr_0f7ecc5d-1c48-4bd3-b490-5ca7850e358d"
# This is the name of the organizer if the user searches for them in VRChat.
name = "Organizer A"

[[join]]
id = "usr_78f6edbc-9e4d-4632-9f19-b1b605234ae5"
name = "Organizer B"
```

The event toml file normally does not contain the name of the event. The event name is the name of the file. However, if the name contains special characters, it can be specified inside the file by using `name = "my/event"` at the top of the file outside of any sections.

The event toml file normally does not contain the name of the poster image either. The poster file name is the same as the name of the event toml file, but with the extension changed to one of `.webp`, `.png`, `.jpg`, `.jpeg`.

[toml]: https://toml.io/

## Non-daily events

If the event is not daily, add sections for the days of the week when it occurs.

```toml
# This event is Monday Wednesday Friday.
[days.monday]
[days.wednesday]
[days.friday]
```

## Overriding details

Inside the day sections, you can override event details just for that day.

```toml
# This event starts at a different time on Monday.
[days.monday]
start = "18:00"
description = "I hate mondays."
```

It's also possible to override event details for different languages.

```toml
# "ja" is an ISO 639-1 language code.
[lang.ja]
name = "私のイベント"
poster = "my event-ja.webp"
```

This also applies to day sections.

```toml
[days.monday.lang.ja]
description = "月曜日が嫌いだ。"
```

## Less common details

```toml
# If the event starts in the future, the start date can be set.
# This is the first day the event is held.
start_date = "2023-06-26"
# If the event has a known end date, the end date can be set.
# This is the last day the event is held.
end_date = "2023-07-31"
```

## Confirmations and cancellations

These are supported by the compiler, but not yet used by the calendar script.

```toml
# For semi-regular events, specify the confirmed dates.
# Dates that are not confirmed will be considered unconfirmed.
confirmed = [
    "2023-06-26",
    "2023-06-27",
]

# Dates when the event would normally be held can be cancelled.
canceled = [
    "2023-06-28",
]
```

# The meta file

There must be a file named `meta.toml` with information about the calendar data.

```toml
title = "My event calendar"
description = "This calendar contains cool events."
link = "https://github.com/nil-vr/example-calendar"
```

As with the events, these details can be overridden for different languages.

```toml
[lang.ja]
title = "私のイベントカレンダー"
description = "このカレンダーではかっこいいイベントがある。"
```

# Compiling the data

The easy way to do this is to follow the example of [wc-undou] and set up [GitHub Actions] to compile the data and publish it to [GitHub Pages] for you.

[GitHub Actions]: https://docs.github.com/actions
[GitHub Pages]: https://docs.github.com/pages

The prebuilt compiler is available from the [GitHub releases page]. It's compiled to [WASM] for [WASI], so you'll need a WASI runtime like [Wasmer] to run it.

[GitHub releases page]: https://github.com/nil-vr/wc-compiler/releases
[WASM]: https://webassembly.org/
[WASI]: https://wasi.dev/
[Wasmer]: https://wasmer.io/

The compiler takes two parameters. First, the name of the input directory containing the toml files and posters, and second, the name of the output directory to save the output json and renamed posters.

```
wc-compiler events out
```

The output directory must be published somewhere that it can be read by VRChat, preferably one of the locations that is [trusted by VRChat][string-loading] (GitHub pages). The output directory must also be saved and reused across builds. If you use a clean directory for every build, users may sometimes see the wrong posters.

[string-loading]: https://creators.vrchat.com/worlds/udon/string-loading/
