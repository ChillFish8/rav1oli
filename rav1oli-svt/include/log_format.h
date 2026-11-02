#ifndef RAV1OLI_SVT_LOG_FORMAT_H
#define RAV1OLI_SVT_LOG_FORMAT_H

// Receives one formatted SVT-AV1 log message. `tag` may be NULL.
typedef void (*rav1oli_svt_log_sink)(int level, const char *tag, const char *message);

// Installs an SVT-AV1 log callback that formats each message and passes it to `sink`.
void rav1oli_svt_install_log(rav1oli_svt_log_sink sink);

#endif
