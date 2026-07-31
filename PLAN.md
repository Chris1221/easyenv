# easyenv tool description

I would like to create a very fast tool called EasyENV which effectively does one thing: when you CD into a directory with a .env file, it loads those environmental variables into your shell, and if you CD out of that directory, it unloads them. If there are .env files in parent directories, it should load those as well, but the child directory's .env file should take precedence over the parent directory's .env file.

Comparison tools are direnv (which does not allow for .env by default and doesn't actually work for me) and autoenv (which doesn't unload the variables when you CD out of the directory without additional configuration). easyenv is supposed to be the single solution to this problem, and it should be very fast and lightweight.

I would like to write this in Rust and distribute a binary that can be easily installed on Linux, MacOS, and Windows. The tool should be simple to use, with minimal configuration required. It should auto detect when you CD into a directory with a .env file and automatically load the variables, and it should also automatically unload them when you CD out of that directory. No messages or confirmation should be required from the user, and it should work seamlessly in the background without any user intervention. Basically, people should forget that they've installed it.

Please create a plan for the development and distribution of easyenv. Focus on 1) speed, and 2) ease of use for a user.