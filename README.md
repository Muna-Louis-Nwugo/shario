# Shario (WIP)
Shario [will be] a real-time code collaboration tool for students, professors, co-workers, and others over a shared WiFi connection. It replaces IDE's propietary code sharing tools and the ridiculously cumbersome workflow of "git commit; git push; git pull" by using a in the working directory with a set of standards allowing any IDE to attach to it. It is ironically inspired by git.

Shario is licensed under the GNU Generl Public License v3. It shall be free to use forever and always.

The name "Shario" is a combination of the word "Share" and "I/O". I know this is supposed to become "Shareio" but I don't like how e and i look next to each other. 

Shario shall be initialized and managed in the terminal using the "shar" command.

## How it works 
The idea behind this is a socket core to which one can mount a plugin or tool for whichever IDE is their preferred. Shario does plan on implementing plugins for VSCode, Jetbrains, and Vim users, but feel free to build your own. As keys are typed into a users computer, the plugins inform shario of the change, and shario distributes that change to all other collaborators.

### Port 
The Shario API accepts packets of 16 bytes:

[UTF-8 keystroke][position][operation][timestamp][emmittter ID]
   [4 bytes]     [2 bytes]  [1 byte]   [8 bytes]   [1 bytes]

This is always the case. Standardization is necessary for compatability across environments. 
# TODO: come up with standards for determining which file and which position in which file
