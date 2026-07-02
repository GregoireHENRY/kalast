#!/usr/bin/env python

OCID_NAME = {
    -3: "total primary occultation",
    -2: "annular primary occultation",
    -1: "partial primary occultation",
    0: "no occultation",
    1: "partial secondary occultation",
    2: "annular secondary occultation",
    3: "total secondary occultation",
}

ECID_NAME = {
    -3: "total primary eclipse",
    -2: "annular primary eclipse",
    -1: "partial primary eclipse",
    0: "no eclipse",
    1: "partial secondary eclipse",
    2: "annular secondary eclipse",
    3: "total secondary eclipse",
}


date_impact = "2022-09-26"

# timeline to cover
# date_start = "2022-08-05"
# date_end = "2023-01-19"

# sep
# date_start = "2022-08-05 03:00"
# date_end = "2022-08-05 06:00"

# pep
# date_start = "2022-08-05 09:00"
# date_end = "2022-08-05 12:00"

# pep
# les makes 21:46 - 22:53, 0.033, complete
# date_start = "2022-08-05 20:00"
# date_end = "2022-08-06 01:00"
# date_start = "2022-08-05 15:00"
# date_end = "2022-08-10 00:00"

# sep
# les makes 01:42 - 02:52, 0.043, 01:42 - 01:48, partial
# date_start = "2022-08-17 00:00"
# date_end = "2022-08-17 04:00"

# sep
# les makes 00:35 - 01:45, 0.045, complete
# date_start = "2022-08-22 22:00"
# date_end = "2022-08-23 04:00"

# pop
# les makes, 23:39 - 00:21, 0.019, complete
# date_start = "2022-10-21 22:00"
# date_end = "2022-10-22 02:00"

# sep
# date_start = "2022-08-13 01:00"
# date_end = "2022-08-13 05:00"


# "/Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/65803_TN_2022-11-17_Exo.obs"
# JD 2459901.5445567 -- 2459901.7504413
# date_start = "2022-11-17 22:00"
# date_end = "2022-11-18 09:00"

# "/users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/65803_tn_2023-01-18_exo.obs"
# jd 2459963.3663977 -- 2459963.7494823
# sepb    2023.01.18 18:37    2459963.2763
# sopb    2023.01.18 19:13    2459963.3013
# sope    2023.01.18 19:44    2459963.3225     0.043
# sepe    2023.01.18 19:47    2459963.3246     0.043
#
# pepb    2023.01.19 00:19    2459963.5138
# popb    2023.01.19 00:55    2459963.5384
# pope    2023.01.19 01:25    2459963.5592     0.042
# pepe    2023.01.19 01:28    2459963.5613     0.042
#
# sepb    2023.01.19 05:59    2459963.7496
# sopb    2023.01.19 06:36    2459963.7750
# sope    2023.01.19 07:07    2459963.7967     0.042
# sepe    2023.01.19 07:08    2459963.7975     0.043
# date_start = "2023-01-18 16:00"
# date_end = "2023-01-19 09:00"

# "/users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/65803_TN_2022-12-28_Exo.obs"
# jd 2459942.4554262      2459942.7086523
#    2022-12-28 22:55:49  2022-12-29 05:00:28
# SOPB    2022.12.28 22:46    2459942.4490
# SEPB    2022.12.28 22:53    2459942.4536
# SETB    2022.12.28 23:17    2459942.4706
# SOPE    2022.12.28 23:23    2459942.4744     0.046
# SETE    2022.12.28 23:42    2459942.4881
# SEPE    2022.12.29 00:07    2459942.5052     0.046
#
# POPB    2022.12.29 04:27    2459942.6861
# PEPB    2022.12.29 04:33    2459942.6902
# POPE    2022.12.29 05:03    2459942.7111     0.053
# PEPE    2022.12.29 05:47    2459942.7410     0.053
# date_start = "2022-12-28 22:00"
# date_end = "2022-12-29 06:00"

# /Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-08-16_ALCDEF.txt
# jd 2459808.290664        2459808.497131
#    2022-08-16 18:58:33   2022-08-16 23:55:52
# SEPB    2022.08.16 13:48    2459808.0755
# SEPE    2022.08.16 14:57    2459808.1234     0.043
#
# PEPB    2022.08.16 19:45    2459808.3234
# PEPE    2022.08.16 20:51    2459808.3688     0.027
#
# SEPB    2022.08.17 01:42    2459808.5713
# SEPE    2022.08.17 02:52    2459808.6196     0.043
# date_start = "2022-08-16 18:58"
# date_end = "2022-08-16 23:56"


# /Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-08-24_ALCDEF.txt
# jd 2459815.41744        2459815.57088
#    2022-08-23 22:01:07  2022-08-24 01:42:04
# PEPB    2022.08.23 18:27    2459815.2694
# PEPE    2022.08.23 19:34    2459815.3157     0.024
#
# SEPB    2022.08.24 00:24    2459815.5169
# SEPE    2022.08.24 01:35    2459815.5661     0.046
#
# PEPB    2022.08.24 06:21    2459815.7652
# PEPE    2022.08.24 07:29    2459815.8119     0.024
# date_start = "2022-08-23 22:01"
# date_end = "2022-08-24 01:43"


# /Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-08-26_ALCDEF.txt
# jd 2459817.239565        2459817.569205
#    2022-08-25 17:44:58   2022-08-26 01:39:39
# SEPB    2022.08.25 12:07    2459817.0052
# SEPE    2022.08.25 13:18    2459817.0544     0.047
#
# PEPB    2022.08.25 18:05    2459817.2535
# PEPE    2022.08.25 19:12    2459817.3006     0.023
#
# SEPB    2022.08.26 00:02    2459817.5014
# SEPE    2022.08.26 01:12    2459817.5506     0.047
#
# PEPB    2022.08.26 05:59    2459817.7498
# PEPE    2022.08.26 07:06    2459817.7964     0.022
# date_start = "2022-08-25 17:44"
# date_end = "2022-08-26 01:40"

# /Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-09-13_ALCDEF.txt
# jd 2459836.362031        2459836.496064
#    2022-09-13 20:41:19   2022-09-13 23:54:20
# PEPB    2022.09.13 14:29    2459836.1036
# PEPE    2022.09.13 15:35    2459836.1498     0.015
#
# SEPB    2022.09.13 20:24    2459836.3502
# SETB    2022.09.13 20:51    2459836.3690
# SETE    2022.09.13 21:15    2459836.3856
# SEPE    2022.09.13 21:41    2459836.4040     0.051
#
# PEPB    2022.09.14 02:23    2459836.5994
# PEPE    2022.09.14 03:29    2459836.6456     0.015
# date_start = "2022-09-13 20:41"
# date_end = "2022-09-13 23:55"

# /Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-09-17_ALCDEF.txt
# jd 2459840.249767        2459840.477033
#    2022-09-17 17:59:40   2022-09-17 23:26:56
# PEPB    2022.09.17 13:41    2459840.0706
# PEPE    2022.09.17 14:48    2459840.1168     0.014
#
# SEPB    2022.09.17 19:37    2459840.3177
# SETB    2022.09.17 20:02    2459840.3352
# SETE    2022.09.17 20:32    2459840.3556
# SEPE    2022.09.17 20:56    2459840.3727     0.051
#
# PEPB    2022.09.18 01:35    2459840.5664
# PEPE    2022.09.18 02:42    2459840.6131     0.014
date_start = "2022-09-17 17:59"
date_end = "2022-09-17 23:27"
