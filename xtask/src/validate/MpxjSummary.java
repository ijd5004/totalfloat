// SPDX-License-Identifier: Apache-2.0
//
// Prints what MPXJ reads from a schedule file, one `key value` line each, for
// `cargo xtask validate`. Run with the MPXJ jars on the class path:
//   java -cp "mpxj.jar:lib/*" MpxjSummary.java FILE
// MPXJ is used as a separate tool, readers only; its scheduling code is never
// called (ARCHITECTURE.md, Scheduling rules).
import java.io.File;
import org.mpxj.ProjectFile;
import org.mpxj.Task;
import org.mpxj.reader.UniversalProjectReader;

public class MpxjSummary {
    public static void main(String[] args) throws Exception {
        ProjectFile project = new UniversalProjectReader().read(new File(args[0]));
        if (project == null) {
            System.err.println("MPXJ doesn't recognise this file's format");
            System.exit(2);
        }
        long activities = 0;
        long summaries = 0;
        long links = 0;
        for (Task task : project.getTasks()) {
            Integer id = task.getUniqueID();
            // Unique ID 0 is the project summary task MPXJ adds.
            if (id == null || id == 0) {
                continue;
            }
            if (task.getSummary()) {
                summaries++;
            } else {
                activities++;
            }
            links += task.getPredecessors().size();
        }
        long resources = project.getResources().stream()
            .filter(r -> r.getUniqueID() != null && r.getUniqueID() != 0)
            .count();
        long assignments = project.getResourceAssignments().stream()
            .filter(a -> a.getResource() != null)
            .count();
        System.out.println("activities " + activities);
        System.out.println("summaries " + summaries);
        System.out.println("links " + links);
        System.out.println("resources " + resources);
        System.out.println("assignments " + assignments);
        System.out.println("calendars " + project.getCalendars().size());
    }
}
