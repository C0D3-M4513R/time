# time
This repository is a binary, that writes the current time to a file.
The produced executable will expect a path to a file.
In that file specified the current time will be written.

## Known potential problems
- On Windows the program will check for changes to the timezone every minute.
- On Platforms other than Windows the program will always write UTC to the file.

Example commandline: `time.exe "C:\Users\some-user\Documents\time.txt"`