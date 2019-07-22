# gerrit-stats

A simple tool to fetch user statistics from Gerrit. For each user defined in the config file, this tool will
grab following stats:
* Changes (CH) - Total number of changes that got merged
* Approvals (AP) - Total number of approved changes (only +2's)
* Commends Made (CM) - Total number of comments made on other user reviews (doesn't count on your own)
* Comments Received (CR) - Total number of comments received from other users on your reviews
* Comments Received per Change (CR/CH) - Average number of comments received from other users on your reviews
* Commit Words (CW) - Total number of words in commit messages in all changes
* Commit Words per Change (CW/CH) - Average number of words in commit message per change
* Patch Sets (PS) - Total number of patch sets created
* Patch Sets per Change (PS/CH) - Average number of patch sets per change

Note that some of the statistics won't make sense if the users work on different projects, or they don't participate
in each others reviews. For example, _Comments Made_ is searched through other users reviews. If the user made
comments on reviews of users that are not specified in the config, then these won't be found.

## Usage

It is assumed that you have Rust installed on your system. Building this tool only requires one command:

`cargo build`

To run the tool you'll need to supply a config file that specifies all the necessary data, see example.toml for
an example of config file - it should be self-explanatory. Once you have it, just pass config file and username
that can authenticate with the server in the config file (you might be prompted to enter password).

`./gerrit-stats --config=example.toml --user=radszy`

The output CSV file will be generated in the same directory as the binary file.
