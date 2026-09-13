# Test 1 - Login and Auth -> SUCCESS

Tried to login, register and auth and used the command "users" to see if resulted correctly

# Test 2 - disconnection -> SUCCESS AFTER FIX

Tried to exit a console from a client abruptedly, waited 3:00 minutes to see if "users" command won't report anymore the connected user.
## FIX
Added methods remove_trip at the end after the loop breaks: disconnection is instant now.

# Test 3 - status revealing: see if the status changes to "Still" after 3 minutes (6 steps) -> SUCCESS
Created Steady-Version.csv to test if the user gets "Still" status after the 8th log

# Test 4 - Stats on users are correctedly inserted in DB => SUCCESS
Check on DB Browser if the id "8" is printed after finishing the csv
Passed only if the client sent the "completed" signal.

# Test 5 - Stats on users are computed correctly -> SUCCESS on DAY period
Success on DAY period and probably correct on WEEK period for pause time, distance and total time.

# Test 6 - Messages to the user and broadcast -> SUCCESS
Success both per-user and broadcast messages.
## NOTE: 
If DBBrowser or other software keeps DB occupied in the meanwhile, it can raise error.

# Test 7 - Logging time of the hardware consumption statistics -> SUCCESS (with caveaut)
## NOTE: 
Even if it makes sense also controlling resource manager, it print "0" as CPU consumption:
[2026-08-23 19:22:00] PID: 178456 | CPU Usage: 0.00% | Uptime process: 1s
[2026-08-23 19:24:00] PID: 178456 | CPU Usage: 0.00% | Uptime process: 121s
[2026-08-23 19:26:00] PID: 178456 | CPU Usage: 0.00% | Uptime process: 241s
[2026-08-23 19:28:00] PID: 178456 | CPU Usage: 0.00% | Uptime process: 361s
[2026-08-23 19:30:00] PID: 178456 | CPU Usage: 0.00% | Uptime process: 481s
[2026-08-23 19:32:00] PID: 178456 | CPU Usage: 0.00% | Uptime process: 601s

