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

### Shar 
The shar is the core of shario. It is made up of three parts:
1. The operation queue
  - queues operations received over the network
2. the operation buffer
  - stores operation to be broadcast to collaborators
3. the operation log
   - save the last 50 operations made

### terminal commands

- shar make: Initializes a .shar/ directory in the current folder, generates a unique Session ID, and spawns the background daemon (shar). This establishes the local machine as the Host (Single Source of Truth).

- shar join <session-id>:Connects a client to an active host on the local network. It triggers the Hydration process: the client receives a full-state snapshot of the codebase followed by the live binary diff stream.

- shar terminate: Safely shuts down the background daemon, flushes any remaining 8KB buffers to the binary diffs.log, and removes the session.lock file.
