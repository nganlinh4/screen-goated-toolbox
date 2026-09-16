package dev.screengoated.toolbox.mobile.service.preset;

import android.app.Activity;
import android.os.Bundle;
import android.widget.EditText;
import android.widget.LinearLayout;

/** An independent editor surface in the instrumentation package. */
public final class DictationEditorActivity extends Activity {
    @Override public void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        layout.setPadding(32, 48, 32, 32);
        for (String label : new String[] {"First editor", "Second editor"}) {
            EditText editor = new EditText(this);
            editor.setContentDescription(label);
            editor.setHint(label);
            editor.setMinLines(3);
            layout.addView(editor, new LinearLayout.LayoutParams(-1, -2));
        }
        setContentView(layout);
    }
}
