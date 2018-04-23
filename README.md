# Terminal Spectrograph

This is a spectrum view and waterfall plot, all in the terminal!

[![](http://i.imgur.com/zdShfGf.jpg)](https://www.youtube.com/watch?v=wT1ATV_WEEo)

# How is it done?
The spectrum view is done by printing braille characters à la [drawille](https://github.com/asciimoo/drawille).
This gives twice the horizontal resolution and four times the vertical resolution of the terminal cells.
The waterfall is done by plotting the "upper half block" character (▀) with a different background and foreground color,
giving twice the vertical resolution of the cells. You'll need a terminal with 256-color support for
the colors to work properly.

The FFTs are done with the [RustFFT](https://github.com/awelkie/rustfft) library, and the terminal UI is done using the
[rustty](https://github.com/cpjreynolds/rustty) library.

# Radio
Currently, this project only works with the HackRF. Support for other radios should be coming soon.

## Hackrf Dependencies
Ubuntu 16.04: ```libhackrf-dev```
