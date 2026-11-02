use super::*;
use varta_common::{
    http::MAX_REPORTED_REQUESTS,
    language::LanguageProfile as P,
    permission::{DecisionSource, MAX_AUDIT_ENTRIES, PermissionState},
};

#[test]
fn http_v2_reports_failed_and_successful_workers_with_bounded_audit_and_full_totals() {
    let source = r#"
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse>{
            console.log("request "+r.target);
            for(let i=0;i<12;i++){try{await fetch("http://127.0.0.1:1/denied");}catch(error){}}
            if(r.target=="/fail")throw new TypeError("request failed");
            return {status:200,body:"ok",headers:[]};
        }
        export function main():void{console.log("startup");}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let server = Server::start_profile(source, P::CheckedF64Utf16V38);
    let count = MAX_REPORTED_REQUESTS + 2;
    for index in 0..count {
        let target = if index == 1 {
            "/fail".to_owned()
        } else {
            format!("/{index}")
        };
        assert_eq!(
            server.request("GET", &target, b"").0,
            if index == 1 { 500 } else { 200 }
        );
    }
    let events = server.requests.clone();
    let final_report = server.report.clone();
    server.finish();
    let events = events.lock().unwrap();
    assert_eq!(events.len(), count);
    for (index, request) in events.iter().enumerate() {
        assert_eq!(request.sequence, index as u64 + 1);
        assert_eq!(
            request.outcome,
            if index == 1 {
                HttpRequestOutcome::Failed
            } else {
                HttpRequestOutcome::Returned { status: 200 }
            }
        );
        let usage = request.budget.as_ref().unwrap();
        assert!(!usage.per_turn && !usage.deadline_per_turn);
        assert_eq!(usage.total_turns, 1);
        assert_eq!(usage.turns.len(), 1);
        assert_eq!(usage.turns[0].kind, TurnKind::Execution);
        assert!(usage.total_fuel > 0 && usage.peak_memory_bytes > 0 && usage.output_bytes > 0);
        assert!(usage.turns[0].microtasks > 0 && usage.turns[0].host_calls >= 12);
        assert!(
            request
                .audit
                .iter()
                .any(|entry| entry.operation == "net.request"
                    && entry.source == DecisionSource::Policy
                    && entry.outcome == PermissionState::Denied)
        );
    }
    let final_report = final_report.lock().unwrap();
    let report = final_report.as_ref().unwrap();
    assert_eq!(report.total_requests, count as u64);
    assert_eq!(
        (report.returned, report.failed, report.cancelled),
        (count as u64 - 1, 1, 0)
    );
    assert_eq!(report.requests.len(), MAX_REPORTED_REQUESTS);
    assert_eq!(report.requests_dropped, 2);
    let retained = report.requests.iter().map(|r| r.audit.len()).sum::<usize>();
    assert_eq!(retained, MAX_AUDIT_ENTRIES);
    let all_audit = events
        .iter()
        .map(|r| r.audit.len() as u64 + r.audit_dropped)
        .sum::<u64>();
    assert_eq!(report.audit_dropped, all_audit - retained as u64);
    assert_eq!(
        report.total_fuel,
        events
            .iter()
            .map(|r| r.budget.as_ref().unwrap().total_fuel)
            .sum::<u64>()
    );
    assert_eq!(
        report.total_output_bytes,
        events
            .iter()
            .map(|r| r.budget.as_ref().unwrap().output_bytes as u64)
            .sum::<u64>()
    );
    assert_eq!(
        report.total_host_calls,
        events
            .iter()
            .map(|r| u64::from(r.budget.as_ref().unwrap().turns[0].host_calls))
            .sum::<u64>()
    );
    assert_eq!(
        report.total_microtasks,
        events
            .iter()
            .map(|r| r.budget.as_ref().unwrap().turns[0].microtasks)
            .sum::<u64>()
    );
    assert_eq!(
        report.total_wall,
        events
            .iter()
            .map(|r| r.budget.as_ref().unwrap().turns[0].wall)
            .sum::<Duration>()
    );
    assert_eq!(
        report.peak_memory_bytes,
        events
            .iter()
            .map(|r| r.budget.as_ref().unwrap().peak_memory_bytes)
            .max()
            .unwrap()
    );
}

#[test]
fn legacy_http_does_not_publish_v2_request_reports() {
    let server = Server::start(HANDLER);
    assert_eq!(server.request("GET", "/", b"").0, 200);
    let events = server.requests.clone();
    let report = server.report.clone();
    server.finish();
    assert!(events.lock().unwrap().is_empty());
    assert!(report.lock().unwrap().is_none());
}
