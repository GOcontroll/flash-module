v2.0.0
 - Firmware locations have been moved to /lib/firmware/gocontroll/
 - The modules file has been moved to /lib/gocontroll/modules
 - Now errors if it cannot find any firmwares instead of quietly failing

v1.1.0
 - Now wipes the module firmware if it is corrupted so it will try again with a `go-modules update all` call

v1.0.1
 - Fixed Moduline Display match string

v1.0.0
 - First release
 - Can work with the old slow bootloader aswell as the new fast one