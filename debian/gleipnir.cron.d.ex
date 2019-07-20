#
# Regular cron jobs for the gleipnir package
#
0 4	* * *	root	[ -x /usr/bin/gleipnir_maintenance ] && /usr/bin/gleipnir_maintenance
